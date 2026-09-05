//! The host side of one episode: a journal, a roster, and the step loop.
//!
//! This is the whole contract a consuming host has to honour, and it is worth
//! reading as the shortest statement of it: fold, run the single turn the
//! library authorized, append that turn, commit the returned state, repeat.
//! The library never appends, never waits, and never calls back into the host.

use std::time::{Duration, Instant};

use tinyhivemind_hive::{
    BidReason, Conversation, Directory, DirectoryPolicy, EpisodePolicy, EpisodeState, HiveStep,
    HiveTurn, Phase, Sequence, SessionAuthor, SessionMessage,
    desk::{Desk, DeskSet, ResponderMode},
    directory, project_for,
    roster::{Roster, RosterMember},
    step,
    trace::{TopicId, Trace, TraceKind, resolve},
};

use crate::metrics::spearman_milli;
use crate::sim::{Room, SimAgent};

/// Anything that can fill one authorized turn.
///
/// Simulated participants and a real agent CLI differ only here, which is the
/// point: the protocol underneath is the same one either way.
pub(crate) trait Participant {
    /// Canonical agent id, matching a desk member.
    fn id(&self) -> &str;

    /// Produce the body of one turn, given exactly what it may see.
    ///
    /// # Errors
    ///
    /// Returns a host-side failure, such as an agent process that did not
    /// answer.
    fn speak(&mut self, turn: &HiveTurn, visible: &[&SessionMessage]) -> Result<String, String>;

    /// What one of this participant's turns costs, for the vote arm's charge
    /// and a deliberation's own `cost_units` total. A live agent costs the
    /// same as any other by default.
    fn cost_unit(&self) -> u32 {
        1
    }
}

/// The desk every simulated room deliberates on.
pub(crate) const DESK_ID: &str = "engineering";
/// Its display name.
pub(crate) const DESK_NAME: &str = "Engineering";

/// How an episode ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Ending {
    /// One topic carried and the room recorded it.
    Converged,
    /// Two or more topics carried and nobody could break the tie.
    Deadlocked,
    /// The turn budget ran out first.
    Exhausted,
    /// Nobody's urge cleared their threshold.
    Idle,
}

impl Ending {
    /// A fixed-width label for the tables.
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Converged => "converged",
            Self::Deadlocked => "deadlocked",
            Self::Exhausted => "exhausted",
            Self::Idle => "idle",
        }
    }
}

/// What one episode cost and what it decided.
#[derive(Clone, Debug)]
pub(crate) struct EpisodeReport {
    /// How it ended.
    pub(crate) ending: Ending,
    /// The topic it settled on, if it settled.
    pub(crate) decided: Option<TopicId>,
    /// Whether that topic is the genuinely best one.
    pub(crate) correct: bool,
    /// Turns actually taken.
    pub(crate) turns: u32,
    /// Calls into [`step`], including the terminal one.
    pub(crate) step_calls: u32,
    /// Time spent inside the library, excluding the simulated agents.
    pub(crate) library_time: Duration,
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
    /// directory folded from `traces` at [`DirectoryPolicy::DEFAULT`] --
    /// always `DEFAULT`, whatever the episode policy asked for, so the number
    /// is comparable across arms that fold a directory and arms that do not
    /// -- and the number of turns it took. `None` for a room of fewer
    /// than two members, where a rank correlation is undefined.
    pub(crate) rho_milli: Option<i64>,
}

/// A host-owned append-only journal.
pub(crate) struct Host {
    journal: Vec<SessionMessage>,
    members: Vec<RosterMember>,
    desks: Vec<Desk>,
    retired: Vec<String>,
    conversation: Conversation,
}

impl Host {
    /// Open a host over one desk holding `member_ids`.
    pub(crate) fn new(member_ids: &[&str]) -> Self {
        Self {
            journal: Vec::new(),
            members: member_ids
                .iter()
                .map(|id| RosterMember {
                    id: (*id).to_owned(),
                    name: Some((*id).to_owned()),
                })
                .collect(),
            desks: vec![Desk {
                id: DESK_ID.to_owned(),
                name: DESK_NAME.to_owned(),
                description: Some("Decide the rollout".to_owned()),
                members: member_ids.iter().map(|id| (*id).to_owned()).collect(),
                responder_mode: ResponderMode::Auto,
            }],
            retired: Vec::new(),
            conversation: Conversation {
                desk_id: DESK_ID.to_owned(),
                desk_name: DESK_NAME.to_owned(),
                thread_root: None,
            },
        }
    }

    /// The borrowed roster snapshot.
    pub(crate) fn roster(&self) -> Roster<'_> {
        Roster::new(&self.members, &[], &self.retired)
    }

    /// The borrowed desk snapshot.
    pub(crate) fn desks(&self) -> DeskSet<'_> {
        DeskSet::new(&self.desks, &[], &[], &[], &self.retired)
    }

    /// The conversation the episode runs on.
    pub(crate) fn conversation(&self) -> Conversation {
        self.conversation.clone()
    }

    /// The sequence of the last appended message.
    pub(crate) fn watermark(&self) -> Sequence {
        self.journal
            .last()
            .map_or(Sequence(0), |message| message.sequence)
    }

    fn append(&mut self, author: SessionAuthor, content: String) -> Sequence {
        let next = u64::try_from(self.journal.len())
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        let sequence = Sequence(next);
        self.journal.push(SessionMessage {
            sequence,
            author,
            content,
        });
        sequence
    }

    /// Append an operator message.
    pub(crate) fn operator(&mut self, content: &str) -> Sequence {
        self.append(SessionAuthor::Operator, content.to_owned())
    }

    /// Append an agent turn.
    fn agent(&mut self, id: &str, content: String) -> Sequence {
        self.append(
            SessionAuthor::Agent {
                id: id.to_owned(),
                label: id.to_owned(),
            },
            content,
        )
    }
}

/// Run one full episode over a simulated room.
///
/// # Errors
///
/// Returns the library's own error text if a snapshot or policy is malformed,
/// which in this benchmark can only mean the harness built one wrongly.
pub(crate) fn run_episode(
    room: &Room,
    policy: &EpisodePolicy,
    task: &str,
    keep_trace: bool,
) -> Result<EpisodeReport, String> {
    run_episode_with(room, policy, task, keep_trace, 0)
}

/// Run one full episode, letting every member spend up to `defer_cap` turns
/// on `!defer` instead of arguing outside its own specialty.
///
/// `0` turns the move off, which is what every arm that is not a deferring
/// arm passes: a member that never defers behaves exactly as it did before
/// the move existed, so a deferring arm differs from its control in one thing
/// rather than in two.
///
/// # Errors
///
/// Returns the library's own error text if a snapshot or policy is malformed.
pub(crate) fn run_episode_with(
    room: &Room,
    policy: &EpisodePolicy,
    task: &str,
    keep_trace: bool,
    defer_cap: u32,
) -> Result<EpisodeReport, String> {
    let ids = room.member_ids();
    let mut agents: Vec<SimAgent> = room.agents.clone();
    for agent in &mut agents {
        agent.set_quorum(policy.quorum);
        agent.set_defer_cap(defer_cap);
    }
    let mut participants: Vec<&mut dyn Participant> = agents
        .iter_mut()
        .map(|agent| agent as &mut dyn Participant)
        .collect();
    let report = drive(&ids, &mut participants, policy, task, keep_trace)?;

    // `drive` stays ignorant of which member is an expert or a decisive
    // hidden-profile holder; only the room knows that, and only after the
    // episode has already ended is it safe to ask.
    let expert = room.deciding_expert();
    // In time, or not at all. A deposit at or after the commit boundary is
    // compute the room paid for and could not use, and scoring it as a hit
    // would report exactly the failure this arm exists to find.
    let fact_at = expert
        .and_then(|id| {
            report
                .first_deposit
                .iter()
                .find(|(held, _)| held == id)
                .map(|(_, turn)| *turn)
        })
        .filter(|at| report.commit_at.is_none_or(|boundary| *at < boundary));

    let report = EpisodeReport {
        correct: report.decided.as_ref() == Some(&room.truth),
        has_expert: expert.is_some(),
        decisive: expert.map(str::to_owned),
        fact_deposited: fact_at.is_some(),
        fact_at,
        ..report
    };
    debug_check(&report, &ids, room);
    Ok(report)
}

/// Cheap consistency checks over a freshly built report, active only in
/// debug builds.
///
/// These are bookkeeping invariants across the fields `drive` and
/// `run_episode` fill, not library behaviour -- they exist to catch an
/// accounting bug here before it reaches a published number.
fn debug_check(report: &EpisodeReport, member_ids: &[&str], room: &Room) {
    debug_assert!(
        room.planted.as_ref() != Some(&room.truth),
        "a hidden-profile decoy must never be the genuinely best option"
    );
    debug_assert_eq!(
        report
            .speech
            .iter()
            .map(|(_, turns)| u64::from(*turns))
            .sum::<u64>(),
        u64::from(report.turns),
        "speech counts must add up to the turns actually taken"
    );
    debug_assert!(
        report.defers <= report.turns,
        "cannot defer more turns than were taken"
    );
    debug_assert!(
        report.turns == 0 || report.cost_units >= u64::from(report.turns),
        "every turn costs at least one unit"
    );
    debug_assert_eq!(
        report
            .speech
            .iter()
            .map(|(id, count)| u64::from(room.cost_of(id)).saturating_mul(u64::from(*count)))
            .sum::<u64>(),
        report.cost_units,
        "cost_units must equal each speaker's own cost times its turns"
    );
    if let Some(at) = report.fact_at {
        debug_assert!(report.fact_deposited, "fact_at implies fact_deposited");
        debug_assert!(at < report.turns, "fact_at must be a turn actually taken");
        debug_assert!(
            report.commit_at.is_none_or(|boundary| at < boundary),
            "a fact counted as deposited must have landed before the commit boundary"
        );
    } else {
        debug_assert!(!report.fact_deposited, "fact_deposited without fact_at");
    }
    debug_assert!(
        report.knows_turns <= report.turns,
        "cannot hand out more Knows turns than were taken"
    );
    if let Some(proposer) = &report.proposer {
        debug_assert!(
            member_ids.contains(&proposer.as_str()),
            "the proposer must be a member of the room"
        );
    }
}

/// Drive one episode to termination against arbitrary participants.
///
/// # Errors
///
/// Returns the library's error text for a malformed snapshot, or a
/// participant's own failure.
pub(crate) fn drive(
    member_ids: &[&str],
    agents: &mut [&mut dyn Participant],
    policy: &EpisodePolicy,
    task: &str,
    keep_trace: bool,
) -> Result<EpisodeReport, String> {
    let mut host = Host::new(member_ids);
    host.operator(task);

    let mut state = EpisodeState::opened(host.conversation(), host.watermark());
    let mut library_time = Duration::ZERO;
    let mut step_calls = 0_u32;
    let mut turns = 0_u32;
    let mut trace = Vec::new();
    let mut tally = Tally::opened(member_ids);

    loop {
        let started = Instant::now();
        let decision = {
            let roster = host.roster();
            let desks = host.desks();
            step(&state, &host.journal, &roster, &desks, policy)
        };
        library_time += started.elapsed();
        step_calls = step_calls.saturating_add(1);

        let (ending, decided) = match decision.map_err(|error| error.to_string())? {
            HiveStep::Speak { turn } => {
                let turn = *turn;
                let visible = project_for(&turn, &host.journal);
                let Some(agent) = agents.iter_mut().find(|agent| agent.id() == turn.agent_id)
                else {
                    return Err(format!("no agent named {}", turn.agent_id));
                };
                let content = agent.speak(&turn, &visible)?;
                if keep_trace {
                    trace.push(format!(
                        "{:>10}  {:<10} {:<6} {:<11} saw {:>2}  {content}",
                        turn.agent_id,
                        format!("{:?}", turn.reason).to_lowercase(),
                        format!("{:?}", turn.visibility).to_lowercase(),
                        format!("{:?}", turn.phase).to_lowercase(),
                        visible.len(),
                    ));
                }
                tally.record(&turn, &content, agent.cost_unit(), turns);
                // Durably append the turn, then commit the state it returned.
                // That ordering is what the `next_state` contract requires.
                host.agent(&turn.agent_id, content);
                state = turn.next_state;
                turns = turns.saturating_add(1);
                continue;
            }
            HiveStep::Converged { topic, .. } => (Ending::Converged, Some(topic)),
            HiveStep::Deadlocked { .. } => (Ending::Deadlocked, None),
            HiveStep::Exhausted { .. } => (Ending::Exhausted, None),
            HiveStep::Idle => (Ending::Idle, None),
        };

        // Read back out of the journal the same way any participant would,
        // rather than tracked as the loop ran: the topic this resolves
        // against is only known once the episode has already decided one.
        let proposer = decided
            .as_ref()
            .and_then(|topic| proposer_of(&host.journal, topic));

        // Folded once, after the episode has ended, and deliberately outside
        // the `library_time` accumulator above: `ns/step` is what a host pays
        // to run the state machine, and this is scoring rather than stepping.
        let traces = journal_traces(&host.journal);
        let folded = directory(
            &traces,
            host.watermark(),
            &DirectoryPolicy::DEFAULT,
            &state.thresholds,
        )
        .map_err(|error| error.to_string())?;
        let rho_milli = circularity(&folded, &tally.speech);

        return Ok(EpisodeReport {
            ending,
            decided,
            correct: false,
            turns,
            step_calls,
            library_time,
            trace,
            proposer,
            has_expert: false,
            decisive: None,
            fact_deposited: false,
            fact_at: None,
            defers: tally.defers,
            knows_turns: tally.knows_turns,
            speech: tally.speech,
            cost_units: tally.cost_units,
            first_deposit: tally.first_deposit,
            commit_at: tally.commit_at,
            first_spoke: tally.first_spoke,
            traces,
            rho_milli,
        });
    }
}

/// The per-turn bookkeeping [`drive`] accumulates as an episode runs.
///
/// Gathered into one record rather than a dozen locals, so the step loop stays
/// readable. Every field is filled from what the authorized turn and the
/// content it produced already say: nothing here knows which member holds what,
/// which is what keeps `drive` usable for a live room as well as a simulated
/// one.
struct Tally {
    /// Turns taken, by speaker, in desk order.
    speech: Vec<(String, u32)>,
    /// The turn index at which each agent first spoke.
    first_spoke: Vec<(String, u32)>,
    /// The turn index at which each agent first deposited a topiced fact.
    first_deposit: Vec<(String, u32)>,
    /// The turn index of the first turn run in [`Phase::Commit`].
    commit_at: Option<u32>,
    /// Turns whose content is a `!defer` line.
    defers: u32,
    /// Turns handed out for [`BidReason::Knows`].
    knows_turns: u32,
    /// What every turn taken cost, summed.
    cost_units: u64,
}

impl Tally {
    /// Open a tally over a desk, with every member seeded at zero turns.
    fn opened(member_ids: &[&str]) -> Self {
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
    fn record(&mut self, turn: &HiveTurn, content: &str, cost: u32, at: u32) {
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
pub(crate) fn journal_traces(journal: &[SessionMessage]) -> Vec<Trace> {
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
fn circularity(folded: &Directory, speech: &[(String, u32)]) -> Option<i64> {
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
fn proposer_of(journal: &[SessionMessage], topic: &TopicId) -> Option<String> {
    journal
        .iter()
        .flat_map(|message| resolve(&message.content, None, &message.author, message.sequence))
        .find(|trace| trace.kind == TraceKind::Propose && trace.topic.as_ref() == Some(topic))
        .and_then(|trace| trace.agent_id().map(str::to_owned))
}
