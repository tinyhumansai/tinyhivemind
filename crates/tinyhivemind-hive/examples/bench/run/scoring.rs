//! Scoring and accounting: the [`EpisodeReport`] shape, the running [`Tally`]
//! that fills most of it, and the small folds read back out of a finished
//! journal once an episode has ended.
//!
//! Nothing here drives a turn or decides who may see what — that is
//! [`super::turns`]'s job. This module answers the question asked only once
//! an episode is over: what did it cost, what did it decide, and was the
//! decision any good.

use std::time::Duration;

use tinyhivemind_hive::{
    BidReason, Directory, DirectoryPolicy, EpisodeState, HiveTurn, Phase, Sequence, SessionAuthor,
    SessionMessage,
    aside::AsidePolicy,
    directory,
    trace::{TopicId, Trace, TraceKind, resolve},
};

use super::turns::mean_context_rows;
use crate::metrics::spearman_milli;

/// What one episode cost and what it decided.
#[derive(Clone, Debug)]
pub(crate) struct EpisodeReport {
    /// How it ended.
    pub(crate) ending: super::Ending,
    /// The topic it settled on, if it settled.
    pub(crate) decided: Option<TopicId>,
    /// Whether that topic is the genuinely best one.
    pub(crate) correct: bool,
    /// Turns actually taken.
    pub(crate) turns: u32,
    /// Rounds actually taken: the episode's **depth**.
    ///
    /// Every turn in one round is authorized against the same transcript and
    /// none of them can read another, so a host running async seats pays one
    /// round of wall clock for all of them. [`Self::turns`] is what the budget
    /// bounds and what every number recorded before ADR 0014 was priced in;
    /// this is what a host actually waits for, and at `round_width: 1` the two
    /// are equal.
    pub(crate) rounds: u32,
    /// Mean rows a member was holding when the episode ended.
    ///
    /// What the arm cost the *window*, as against `turns`, which is what it
    /// cost the floor. The two are the whole point of `--context-sweep`: an
    /// arm can be cheap in turns and ruinous in rows, and until this field
    /// existed the benchmark could only see the first.
    pub(crate) context_rows: f64,
    /// Every round the episode ran, in order, with the transcript rows its
    /// turns could read and how many of them it authorized.
    ///
    /// Recorded as the episode runs rather than reconstructed from
    /// [`Self::turns`] and [`Self::rounds`] afterwards. Those two say the
    /// sample was, say, seven turns over four rounds; they cannot say whether
    /// that was `4,1,1,1` reading a growing transcript or `1,1,1,4` reading a
    /// short one, and the two price very differently in
    /// [`crate::cost::CostModel::price`]. Off-floor exchange rounds appear
    /// here too, since a host waits for those as well.
    pub(crate) shape: Vec<crate::cost::RoundShape>,
    /// Calls into [`tinyhivemind_hive::step`], including the terminal one.
    pub(crate) step_calls: u32,
    /// Time spent inside the library, excluding the simulated agents.
    ///
    /// Includes an off-floor exchange round's own library time (see
    /// [`crate::metrics::Aggregate::episodes_per_second`], which wants the
    /// full library cost a host would pay), but **not** [`Self::step_time`],
    /// which [`crate::metrics::Aggregate::nanos_per_step`] divides by
    /// [`Self::step_calls`] and must stay one call to `step` for exactly one
    /// unit of time, matching the `ns/step` column's documented meaning.
    pub(crate) library_time: Duration,
    /// Time spent inside `step` calls alone, excluding an exchange round's
    /// own library time. The numerator `nanos_per_step` actually divides by
    /// `step_calls` — kept separate from [`Self::library_time`] so an
    /// off-floor arm's exchange rounds do not inflate its reported ns/step
    /// against arms that never call `exchange`.
    pub(crate) step_time: Duration,
    /// One line per turn, for the trace view.
    pub(crate) trace: Vec<String>,
    /// Author of the first `!propose` for the topic the episode decided.
    /// `None` when nothing was decided, or nothing was ever proposed for it.
    pub(crate) proposer: Option<String>,
    /// Whether the room had a member whose expertise the decision hinged on
    /// at all -- a topic specialist, or a hidden profile's fact-holder.
    /// `false` for a uniform room, which has neither.
    pub(crate) has_expert: bool,
    /// That member's id, once the episode has ended. `None` for a uniform
    /// room, which has no such member.
    pub(crate) decisive: Option<String>,
    /// Whether that member put its knowledge on the floor -- deposited a
    /// topiced `!evidence` line -- *before the commit boundary*, while the
    /// room could still act on it.
    ///
    /// Deliberately not "spoke at all". A member's opening `!propose` is a
    /// position, not the fact, and counting it made this column read 100% on
    /// every arm and measure nothing: the question is whether the deciding
    /// knowledge reached the room in time, not whether its holder got a turn.
    pub(crate) fact_deposited: bool,
    /// The turn index of that deposit, when it landed in time.
    pub(crate) fact_at: Option<u32>,
    /// Turns whose content is a `!defer` line.
    pub(crate) defers: u32,
    /// Turns the attention market handed out for [`BidReason::Knows`] -- the
    /// directory's holder of the contested topic, brought out because the
    /// transcript says it knows something it has not said.
    pub(crate) knows_turns: u32,
    /// Turns taken, by speaker, in desk order.
    pub(crate) speech: Vec<(String, u32)>,
    /// The sum of every speaker's own `Participant::cost_unit` across every
    /// turn taken.
    pub(crate) cost_units: u64,
    /// Model calls made **off the floor**, in exchange rounds.
    ///
    /// Members *asked* for a line, not rows appended: a member that declines
    /// costs the same call as one that answers, and a round in which everybody
    /// declines is the most expensive kind per row. Counting rows would report
    /// a price lower than the one actually paid.
    ///
    /// Deliberately not folded into `cost_units`, which is defined as each
    /// speaker's own cost times its turns and is asserted to be exactly that.
    /// This is the separate price of an off-floor exchange, and the table
    /// prints it in a column of its own so it cannot hide inside `cost/ep`.
    pub(crate) contacts: u32,
    /// The turn index at which each agent id first deposited a topiced
    /// `!evidence` line, in first-depositing order.
    ///
    /// Bookkeeping only, and read the same way `first_spoke` is: `drive`
    /// records it from the content it already saw, without knowing which
    /// member's deposit would have mattered.
    pub(crate) first_deposit: Vec<(String, u32)>,
    /// The turn index of the first turn the library ran in [`Phase::Commit`],
    /// if the episode ever got there. Everything deposited at or after it
    /// arrived too late to change the answer.
    pub(crate) commit_at: Option<u32>,
    /// The turn index at which each agent id first spoke, in first-speaking
    /// order.
    ///
    /// Bookkeeping only: `drive` builds it from turns it already sees,
    /// without knowing what an "expert" or a "decisive" member is. A caller
    /// that does know which id matters reads it back afterwards, which is
    /// how `run_episode` fills `expert_spoke` and `expert_at` without
    /// teaching `drive` the room's truth.
    pub(crate) first_spoke: Vec<(String, u32)>,
    /// Every trace the episode's journal carried, read back through the
    /// library's own [`resolve`] once the episode had ended.
    ///
    /// Two callers need them: the directory folded below, and `--history`,
    /// which concatenates several episodes' traces to earn a directory the
    /// `ladder+dir` arm routes on.
    pub(crate) traces: Vec<Trace>,
    /// The episode's directory-circularity number, in thousandths: the
    /// Spearman rank correlation between each member's total weight in the
    /// directory folded from `traces` at
    /// [`tinyhivemind_hive::DirectoryPolicy::DEFAULT`] -- always `DEFAULT`,
    /// whatever the episode policy asked for, so the number is comparable
    /// across arms that fold a directory and arms that do not -- and the
    /// number of turns it took. `None` for a room of fewer than two members,
    /// where a rank correlation is undefined.
    pub(crate) rho_milli: Option<i64>,
}

/// The aside policy every arm that opens a check runs under.
///
/// A pair and nothing wider: the whole question is what one member learns from
/// one peer, and a caucus would confound it with group size. `max_messages` is
/// generous because the harness bounds the exchange by the turn budget it is
/// already charged against, and `must_surface` is off because a reading is not
/// a position — nothing here is being kept from the room's *decision*, which
/// an aside cannot move either way.
pub(super) fn aside_policy(members: usize) -> AsidePolicy {
    AsidePolicy {
        enabled: true,
        max_members: 1,
        max_messages: members.saturating_mul(2),
        must_surface: false,
        require_thread: false,
    }
}

/// The per-turn bookkeeping [`super::drive`] accumulates as an episode runs.
///
/// Gathered into one record rather than a dozen locals, so the step loop stays
/// readable. Every field is filled from what the authorized turn and the
/// content it produced already say: nothing here knows which member holds what,
/// which is what keeps `drive` usable for a live room as well as a simulated
/// one.
pub(super) struct Tally {
    /// Turns taken, by speaker, in desk order.
    pub(super) speech: Vec<(String, u32)>,
    /// The turn index at which each agent first spoke.
    pub(super) first_spoke: Vec<(String, u32)>,
    /// The turn index at which each agent first deposited a topiced fact.
    pub(super) first_deposit: Vec<(String, u32)>,
    /// The turn index of the first turn run in [`Phase::Commit`].
    pub(super) commit_at: Option<u32>,
    /// Turns whose content is a `!defer` line.
    pub(super) defers: u32,
    /// Turns handed out for [`BidReason::Knows`].
    pub(super) knows_turns: u32,
    /// What every turn taken cost, summed.
    pub(super) cost_units: u64,
}

impl Tally {
    /// Open a tally over a desk, with every member seeded at zero turns.
    pub(super) fn opened(member_ids: &[&str]) -> Self {
        Self {
            speech: member_ids.iter().map(|id| ((*id).to_owned(), 0)).collect(),
            first_spoke: Vec::new(),
            first_deposit: Vec::new(),
            commit_at: None,
            defers: 0,
            knows_turns: 0,
            cost_units: 0,
        }
    }

    /// Fold one taken turn in, `at` being the index it was taken at.
    pub(super) fn record(&mut self, turn: &HiveTurn, content: &str, cost: u32, at: u32) {
        if let Some(entry) = self.speech.iter_mut().find(|(id, _)| *id == turn.agent_id) {
            if entry.1 == 0 {
                self.first_spoke.push((turn.agent_id.clone(), at));
            }
            entry.1 = entry.1.saturating_add(1);
        } else {
            self.first_spoke.push((turn.agent_id.clone(), at));
            self.speech.push((turn.agent_id.clone(), 1));
        }
        if content.trim_start().starts_with("!defer") {
            self.defers = self.defers.saturating_add(1);
        }
        if turn.reason == BidReason::Knows {
            self.knows_turns = self.knows_turns.saturating_add(1);
        }
        if turn.phase == Phase::Commit && self.commit_at.is_none() {
            self.commit_at = Some(at);
        }
        // Read through the library's own grammar rather than by matching a
        // prefix: a deposit is what `resolve` says a deposit is, whoever
        // wrote the line.
        if deposits_a_fact(content)
            && !self
                .first_deposit
                .iter()
                .any(|(held, _)| *held == turn.agent_id)
        {
            self.first_deposit.push((turn.agent_id.clone(), at));
        }
        self.cost_units = self.cost_units.saturating_add(u64::from(cost));
    }
}

/// Whether one turn's content states a fact about a named option.
///
/// A topiced `!evidence` line and nothing else: an untopiced deposit supplies
/// grounds without saying what they bear on, and a position is a conclusion
/// rather than the knowledge behind it. Read through the library's own
/// [`resolve`], with a placeholder author and sequence, because neither
/// affects what the grammar makes of the line.
fn deposits_a_fact(content: &str) -> bool {
    resolve(content, None, &SessionAuthor::Operator, Sequence(0))
        .iter()
        .any(|trace| trace.kind == TraceKind::Evidence && trace.topic.is_some())
}

/// Read every trace out of a finished journal, through the library's own
/// [`resolve`].
pub(super) fn journal_traces(journal: &[SessionMessage]) -> Vec<Trace> {
    journal
        .iter()
        .flat_map(|message| resolve(&message.content, None, &message.author, message.sequence))
        .collect()
}

/// The episode's directory-circularity number, in thousandths.
///
/// The rank correlation between each member's total folded directory weight
/// and the number of turns it took. A value near `1000` says the directory
/// reproduces the speaking order and has learned nothing except who talked,
/// which is the hazard `docs/specs/expert-delegation.md` obliges the
/// benchmark to report alongside accuracy.
///
/// Every member of the room is a point, including one that never spoke and
/// one the directory never named: leaving those out would score the
/// correlation only over the members the directory already agreed about.
pub(super) fn circularity(folded: &Directory, speech: &[(String, u32)]) -> Option<i64> {
    if speech.len() < 2 {
        return None;
    }
    let mut weights: Vec<u32> = Vec::with_capacity(speech.len());
    let mut turns: Vec<u32> = Vec::with_capacity(speech.len());
    for (id, taken) in speech {
        let weight: i64 = folded
            .entries()
            .iter()
            .filter(|entry| entry.agent_id == *id)
            .map(|entry| entry.weight)
            .sum();
        weights.push(u32::try_from(weight).unwrap_or(u32::MAX));
        turns.push(*taken);
    }
    Some(spearman_milli(&weights, &turns))
}

/// The author of the first `!propose` naming `topic`, read back out of the
/// journal through the library's own [`resolve`] rather than tracked as the
/// loop ran.
pub(super) fn proposer_of(journal: &[SessionMessage], topic: &TopicId) -> Option<String> {
    journal
        .iter()
        .flat_map(|message| resolve(&message.content, None, &message.author, message.sequence))
        .find(|trace| trace.kind == TraceKind::Propose && trace.topic.as_ref() == Some(topic))
        .and_then(|trace| trace.agent_id().map(str::to_owned))
}

/// What `drive_with`'s loop recorded, as one value.
///
/// Genuinely one thing — everything the loop wrote down as it ran, as against
/// everything [`finished`] folds back out of the journal afterwards — and
/// grouping it is what lets that terminal scoring live in a function of its
/// own rather than inside the loop that produced it.
#[derive(Default)]
pub(super) struct Recorded {
    /// Time spent inside the library, exchange rounds included.
    pub(super) library_time: Duration,
    /// Time spent inside `step` calls alone.
    pub(super) step_time: Duration,
    /// Calls into `step`, including the terminal one.
    pub(super) step_calls: u32,
    /// Turns taken.
    pub(super) turns: u32,
    /// Rounds taken.
    pub(super) rounds: u32,
    /// Off-floor model calls made in exchange rounds.
    pub(super) contacts: u32,
    /// Every round's width and the rows it read, for `cost::CostModel::price`.
    pub(super) shape: Vec<crate::cost::RoundShape>,
    /// One line per turn, for the trace view. Empty unless asked for.
    pub(super) trace: Vec<String>,
}

/// Score a finished episode and assemble its report.
///
/// Split out of [`drive_with`] because it answers a different question from
/// the loop above it: the loop decides who speaks next, and this reads back
/// what the whole thing came to once nobody does. Everything here is a fold
/// over the finished journal rather than a step of the protocol, and none of
/// it is charged to `library_time` -- scoring is not stepping.
///
/// # Errors
///
/// Returns the library's own error text if the finished journal will not fold
/// into a directory.
pub(super) fn finished(
    host: &super::Host,
    agents: &mut [&mut dyn super::Participant],
    state: &EpisodeState,
    outcome: (super::Ending, Option<tinyhivemind_hive::trace::TopicId>),
    tally: Tally,
    recorded: &mut Recorded,
) -> Result<EpisodeReport, String> {
    let (ending, decided) = outcome;
    // Read back out of the journal the same way any participant would, rather
    // than tracked as the loop ran: the topic this resolves against is only
    // known once the episode has already decided one.
    let proposer = decided
        .as_ref()
        .and_then(|topic| proposer_of(&host.journal, topic));

    // Folded once, after the episode has ended, and deliberately outside the
    // `library_time` the loop accumulated: `ns/step` is what a host pays to
    // run the state machine, and this is scoring rather than stepping.
    let traces = journal_traces(&host.journal);
    let folded = directory(
        &traces,
        host.watermark(),
        &DirectoryPolicy::DEFAULT,
        &state.thresholds,
    )
    .map_err(|error| error.to_string())?;
    let rho_milli = circularity(&folded, &tally.speech);

    Ok(EpisodeReport {
        ending,
        decided,
        correct: false,
        turns: recorded.turns,
        rounds: recorded.rounds,
        shape: std::mem::take(&mut recorded.shape),
        context_rows: mean_context_rows(agents),
        step_calls: recorded.step_calls,
        library_time: recorded.library_time,
        step_time: recorded.step_time,
        trace: std::mem::take(&mut recorded.trace),
        proposer,
        has_expert: false,
        decisive: None,
        fact_deposited: false,
        fact_at: None,
        defers: tally.defers,
        knows_turns: tally.knows_turns,
        speech: tally.speech,
        cost_units: tally.cost_units,
        contacts: recorded.contacts,
        first_deposit: tally.first_deposit,
        commit_at: tally.commit_at,
        first_spoke: tally.first_spoke,
        traces,
        rho_milli,
    })
}
