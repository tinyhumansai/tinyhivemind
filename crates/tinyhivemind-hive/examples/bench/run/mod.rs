//! The host side of one episode: a journal, a roster, and the step loop.
//!
//! This is the whole contract a consuming host has to honour, and it is worth
//! reading as the shortest statement of it: fold, run the single turn the
//! library authorized, append that turn, commit the returned state, repeat.
//! The library never appends, never waits, and never calls back into the host.
//!
//! The module is split three ways. This file holds the wiring: the
//! [`Participant`] contract, the host-owned journal in [`Host`], and the
//! `run_episode*`/[`drive`] entry points that thread one episode from an
//! opened [`EpisodeState`] to a finished [`EpisodeReport`]. [`turns`] holds
//! the per-turn machinery those entry points call into — whose audience a
//! line gets, appending a turn, running one off-floor exchange round.
//! [`scoring`] holds the bookkeeping folded once an episode has ended, plus
//! the [`EpisodeReport`] shape it fills in.

use std::time::{Duration, Instant};

use tinyhivemind_hive::{
    Conversation, DirectoryPolicy, EpisodePolicy, EpisodeState, ExchangePolicy, ExchangeState,
    HiveStep, HiveTurn, Sequence, SessionAuthor, SessionMessage,
    aside::Audience,
    desk::{Desk, DeskSet, ResponderMode},
    directory, project_for,
    roster::{Roster, RosterMember},
    step,
};

pub(crate) use crate::sim::{CheckStyle, Room, SimAgent};

mod scoring;
mod turns;

pub(crate) use scoring::EpisodeReport;
use scoring::{Tally, circularity, journal_traces, proposer_of};
use turns::{append_turn, exchange_policy, mean_context_rows, one_exchange, scored, trace_line};

/// The marker a participant writes to ask one peer for its reading.
///
/// Shared with [`crate::sim`], which writes it, so the harness has one
/// spelling of the move rather than two that can drift apart.
pub(crate) const ASIDE_MARKER: &str = "!aside";

/// Anything that can fill one authorized turn.
///
/// Simulated participants and a real agent CLI differ only here, which is the
/// point: the protocol underneath is the same one either way.
pub(crate) trait Participant {
    /// Canonical agent id, matching a desk member.
    fn id(&self) -> &str;

    /// Rows this participant is currently holding in its context window.
    ///
    /// `0` for any participant that does not model a window — a live agent
    /// driven over HTTP has a real one, but this harness does not measure it,
    /// and reporting a made-up number for it would be worse than reporting
    /// none. Only the simulated participant answers this meaningfully.
    fn context_rows(&self) -> usize {
        0
    }

    /// Produce the body of one turn, given exactly what it may see.
    ///
    /// # Errors
    ///
    /// Returns a host-side failure, such as an agent process that did not
    /// answer.
    fn speak(&mut self, turn: &HiveTurn, visible: &[SessionMessage]) -> Result<String, String>;

    /// One private line this participant sends **alongside** the floor move it
    /// just made, under [`AsideMode::Alongside`].
    ///
    /// Called once per turn, immediately after [`Participant::speak`] and over
    /// the same projection. Returning `None` — the default, and what every
    /// arm that is not an alongside one does — leaves the turn exactly as it
    /// was.
    ///
    /// At most one row: the exchange is bounded by the floor it rides on, so a
    /// room of `n` members writes at most `n` aside rows per round and each of
    /// them cost its author a turn it had won anyway.
    fn aside(&mut self, turn: &HiveTurn, visible: &[SessionMessage]) -> Option<String> {
        let _ = (turn, visible);
        None
    }

    /// One private line this participant sends in an **exchange round**,
    /// holding no floor and taking no turn.
    ///
    /// Distinct from [`Participant::aside`] because the contracts differ: an
    /// aside rides on a turn this member is already taking, while this is
    /// asked of a member the library named in a round it is not otherwise
    /// part of. Returning `None` — the default — declines the round.
    fn exchange(&mut self, visible: &[SessionMessage]) -> Option<String> {
        let _ = visible;
        None
    }

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

/// A host-owned append-only journal.
pub(crate) struct Host {
    /// Every message appended so far, in sequence order.
    journal: Vec<SessionMessage>,
    /// The one desk's membership, as a roster.
    members: Vec<RosterMember>,
    /// The one desk this harness ever opens.
    desks: Vec<Desk>,
    /// Always empty: nothing this harness runs ever retires a member.
    retired: Vec<String>,
    /// The conversation every episode in this host runs on.
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

    /// Append one message under an explicit `audience`.
    fn append_to(
        &mut self,
        author: SessionAuthor,
        content: String,
        audience: Audience,
    ) -> Sequence {
        let next = u64::try_from(self.journal.len())
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        let sequence = Sequence(next);
        self.journal.push(SessionMessage {
            sequence,
            author,
            content,
            audience,
            elided: None,
        });
        sequence
    }

    /// Append one desk-visible message.
    fn append(&mut self, author: SessionAuthor, content: String) -> Sequence {
        let next = u64::try_from(self.journal.len())
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        let sequence = Sequence(next);
        self.journal.push(SessionMessage {
            sequence,
            author,
            content,
            audience: Audience::Desk,
            elided: None,
        });
        sequence
    }

    /// Append an operator message.
    pub(crate) fn operator(&mut self, content: &str) -> Sequence {
        self.append(SessionAuthor::Operator, content.to_owned())
    }

    /// Append an agent turn addressed to `audience`.
    fn agent_to(&mut self, id: &str, content: String, audience: Audience) -> Sequence {
        self.append_to(
            SessionAuthor::Agent {
                id: id.to_owned(),
                label: id.to_owned(),
            },
            content,
            audience,
        )
    }
}

/// Whether a pairwise check between two members is private, and to whom.
///
/// The two settings differ in exactly one thing — who may read the content —
/// and cost exactly the same number of turns. That is what makes them a
/// matched pair: any difference in the numbers is the price or the value of
/// *privacy*, and not of asking, of targeting, or of spending two turns.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum AsideMode {
    /// No member opens a check; every arm before this one runs here.
    #[default]
    Off,
    /// The exchange is addressed to one peer, and the desk reads a stub.
    Private,
    /// The identical exchange, in the open, where every member reads it.
    Public,
    /// Members exchange privately **off the floor**, in rounds between turns.
    ///
    /// Nobody takes the floor and no turn is produced: the library's
    /// [`exchange`] fold says whether a round is open and which members it
    /// names, and each of them may append at most one private row. Bounded by
    /// an [`ExchangePolicy`] the caller sets, and priced in the `calls/ep`
    /// column rather than hidden. See ADR 0012.
    OffFloor,
    /// The private exchange again, **riding alongside** the floor move that
    /// authored it rather than replacing one.
    ///
    /// One turn produces two rows: the member's ordinary desk-visible
    /// contribution, and one aside. The episode cannot vote it — it resolves
    /// no trace, folds into no standing, and `spent` counts turns rather than
    /// rows — but it is not invisible to decay: the aside still takes the
    /// next raw sequence in the one shared journal, so `salience::standing`'s
    /// `at - trace.sequence` distance for every later desk trace is measured
    /// against a journal one row longer than the same episode without the
    /// aside would have reached. See the "Known limitation" note on
    /// [ADR 0011][adr11].
    ///
    /// This is not the concurrency [ADR 0002][adr2] rules out: `HiveStep::Speak`
    /// still carries exactly one turn, no two participants ever hold the floor,
    /// and the answer arrives on the peer's own next turn. What rides alongside
    /// is a row, not a turn. See [ADR 0011][adr11] and the invariants pinned in
    /// `episode::test` and `tests/fuzz_invariants.rs`.
    ///
    /// [adr2]: https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/adr/0002-hive-episodes-are-sequential.md
    /// [adr11]: https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/adr/0011-an-aside-rides-alongside-a-turn.md
    Alongside,
}

/// One desk-visible agent message, for the self-checks in `sim.rs`.
///
/// Sequence `1` and no thread: the checks that use it read the author and the
/// body and nothing else.
pub(crate) fn one_agent_message(author: &str, body: &str) -> SessionMessage {
    SessionMessage {
        sequence: Sequence(1),
        author: SessionAuthor::Agent {
            id: author.to_owned(),
            label: author.to_owned(),
        },
        content: body.to_owned(),
        audience: Audience::Desk,
        elided: None,
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
    run_episode_checking(
        room,
        policy,
        task,
        keep_trace,
        defer_cap,
        AsideMode::Off,
        0,
        CheckStyle::PLAIN,
    )
}

/// Run one full episode with an **off-floor exchange**: members contact each
/// other in rounds between turns, taking no floor and producing no turn.
///
/// `contact_cap` is the number of private rows one member may write across the
/// episode, and `0` disables the exchange entirely, leaving the episode
/// bit-identical to a plain [`run_episode`]. The round cap is the turn budget,
/// because the harness opens at most one round per turn.
///
/// `style` is what an answer carries: [`CheckStyle::EXCHANGE`] hands over every
/// reading its author holds, and [`CheckStyle::QUIET`] writes the identical
/// rows on the identical rounds and throws every answer away — the control that
/// separates what an exchange said from what merely writing its rows does to a
/// fold that reads recency off raw sequence distance.
///
/// # Errors
///
/// Returns the library's own error text if a snapshot or policy is malformed.
pub(crate) fn run_episode_exchanging_with(
    room: &Room,
    policy: &EpisodePolicy,
    task: &str,
    keep_trace: bool,
    contact_cap: u32,
    style: CheckStyle,
) -> Result<EpisodeReport, String> {
    let ids = room.member_ids();
    let mut agents: Vec<SimAgent> = room.agents.clone();
    for agent in &mut agents {
        agent.set_quorum(policy.quorum);
        agent.set_defer_cap(0);
        // The cap on the participant side is the library's, so a member never
        // wants a row the round would not have authorized.
        agent.set_aside_cap(contact_cap, style);
        agent.set_peers(&ids);
    }
    let mut participants: Vec<&mut dyn Participant> = agents
        .iter_mut()
        .map(|agent| agent as &mut dyn Participant)
        .collect();
    let report = drive_with(
        &ids,
        &mut participants,
        policy,
        task,
        keep_trace,
        if contact_cap == 0 {
            AsideMode::Off
        } else {
            AsideMode::OffFloor
        },
        exchange_policy(contact_cap, policy.turn_budget),
    )?;
    Ok(scored(room, report))
}

/// Run one full episode, letting every member spend up to `aside_cap` turns
/// asking one peer for a second reading before it commits to a position.
///
/// `mode` decides who may read that exchange: the two settings cost the same
/// turns and write the same words, which is what makes them a matched pair.
/// `style` decides what the check does beyond costing a turn: where it is
/// aimed, whether the answer may carry a fact rather than a number, and
/// whether the answer is taken in at all. See [`CheckStyle`].
///
/// `AsideMode::Off` with `aside_cap: 0` is what every other arm passes, and a
/// member that opens no check behaves exactly as it did before the move
/// existed — the same discipline `defer_cap` follows.
///
/// # Errors
///
/// Returns the library's own error text if a snapshot or policy is malformed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_episode_checking(
    room: &Room,
    policy: &EpisodePolicy,
    task: &str,
    keep_trace: bool,
    defer_cap: u32,
    aside_mode: AsideMode,
    aside_cap: u32,
    style: CheckStyle,
) -> Result<EpisodeReport, String> {
    let ids = room.member_ids();
    let mut agents: Vec<SimAgent> = room.agents.clone();
    for agent in &mut agents {
        agent.set_quorum(policy.quorum);
        agent.set_budget(room.budget);
        agent.set_defer_cap(defer_cap);
        agent.set_aside_cap(aside_cap, style);
        agent.set_peers(&ids);
    }
    let mut participants: Vec<&mut dyn Participant> = agents
        .iter_mut()
        .map(|agent| agent as &mut dyn Participant)
        .collect();
    let report = drive_with(
        &ids,
        &mut participants,
        policy,
        task,
        keep_trace,
        aside_mode,
        // The checking arms take no off-floor round; their private rows ride
        // alongside a turn or replace one.
        ExchangePolicy::DEFAULT,
    )?;

    Ok(scored(room, report))
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
    drive_with(
        member_ids,
        agents,
        policy,
        task,
        keep_trace,
        AsideMode::Off,
        ExchangePolicy::DEFAULT,
    )
}

/// What one round writes down as it runs, so [`run_round`] takes one argument
/// for it rather than four.
struct RoundLog<'a> {
    /// The turn-by-turn trace, when `--trace` asked for one.
    trace: Option<&'a mut Vec<String>>,
    /// Per-member accounting for the episode's report.
    tally: &'a mut Tally,
    /// Turns spent so far, across every round.
    turns: &'a mut u32,
}

/// Compose and append every turn in one round, returning the last of them.
///
/// Every member in the round is authorized against the journal as the library
/// folded it, and none of them can read another's row from this same round:
/// `project_for` withholds anything above `round_start`. So appending as we go
/// is safe, and the concurrency is a real property of the projection rather
/// than something this loop arranges.
///
/// The returned turn is the one the exchange round below projects at, which is
/// why the round is never allowed to be empty.
///
/// # Errors
///
/// Returns an error if the round names a member with no agent, or is empty.
fn run_round(
    host: &mut Host,
    agents: &mut [&mut dyn Participant],
    round: &[HiveTurn],
    log: &mut RoundLog<'_>,
    aside_mode: AsideMode,
    members: usize,
) -> Result<HiveTurn, String> {
    let mut last = None;
    for turn in round {
        let visible = project_for(turn, &host.journal);
        let Some(agent) = agents.iter_mut().find(|agent| agent.id() == turn.agent_id) else {
            return Err(format!("no agent named {}", turn.agent_id));
        };
        let content = agent.speak(turn, &visible)?;
        // An alongside aside is asked for over the same projection the floor
        // move was composed from, before either row is appended, so the
        // private line sees exactly what the public one saw.
        let private = if aside_mode == AsideMode::Alongside {
            agent.aside(turn, &visible)
        } else {
            None
        };
        if let Some(trace) = log.trace.as_deref_mut() {
            trace.push(trace_line(turn, &content, visible.len()));
        }
        log.tally
            .record(turn, &content, agent.cost_unit(), *log.turns);
        append_turn(host, turn, content, private, aside_mode, members);
        last = Some(turn.clone());
        *log.turns = log.turns.saturating_add(1);
    }
    last.ok_or_else(|| "a speaking round is never empty".to_owned())
}

/// [`drive`], with pairwise checks enabled.
///
/// # Errors
///
/// Returns the library's own error text if a snapshot or policy is malformed.
pub(crate) fn drive_with(
    member_ids: &[&str],
    agents: &mut [&mut dyn Participant],
    policy: &EpisodePolicy,
    task: &str,
    keep_trace: bool,
    aside_mode: AsideMode,
    exchange: ExchangePolicy,
) -> Result<EpisodeReport, String> {
    let mut host = Host::new(member_ids);
    host.operator(task);

    let mut state = EpisodeState::opened(host.conversation(), host.watermark());
    let mut library_time = Duration::ZERO;
    let mut step_time = Duration::ZERO;
    let mut step_calls = 0_u32;
    let mut turns = 0_u32;
    // Rounds, not turns, are the episode's *depth*: every turn in one round is
    // authorized against the same transcript and none of them can read another,
    // so a host running async seats pays one round of wall clock for all of
    // them. `turns` stays beside it because it is what the budget bounds and
    // what every recorded number before ADR 0014 was priced in.
    let mut rounds = 0_u32;
    // Calls made in exchange rounds, and the round count carried across them.
    // The count is the host's because a round nobody wrote in leaves no row to
    // fold it back out of, and that round still cost its calls.
    let mut contacts = 0_u32;
    let mut opened = ExchangeState::opened();
    let mut trace = Vec::new();
    let mut tally = Tally::opened(member_ids);

    loop {
        let started = Instant::now();
        let decision = {
            let roster = host.roster();
            let desks = host.desks();
            step(&state, &host.journal, &roster, &desks, policy)
        };
        let elapsed = started.elapsed();
        library_time += elapsed;
        step_time += elapsed;
        step_calls = step_calls.saturating_add(1);

        let (ending, decided) = match decision.map_err(|error| error.to_string())? {
            HiveStep::Speak {
                turns: round,
                next_state,
            } => {
                let last = run_round(
                    &mut host,
                    agents,
                    &round,
                    &mut RoundLog {
                        trace: keep_trace.then_some(&mut trace),
                        tally: &mut tally,
                        turns: &mut turns,
                    },
                    aside_mode,
                    member_ids.len(),
                )?;
                state = *next_state;
                rounds = rounds.saturating_add(1);

                // An exchange round, between turns and never during one. The
                // library says whether one is open and who it names; the
                // episode's own state is already committed and does not move
                // for any of it.
                if aside_mode == AsideMode::OffFloor {
                    let members = member_ids.len();
                    let (ran, spent) =
                        one_exchange(&mut host, agents, &last, &state, &exchange, opened, members)?;
                    contacts = contacts.saturating_add(ran.calls);
                    opened = ran.next;
                    library_time += spent;
                }
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
            context_rows: mean_context_rows(agents),
            step_calls,
            library_time,
            step_time,
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
            contacts,
            first_deposit: tally.first_deposit,
            commit_at: tally.commit_at,
            first_spoke: tally.first_spoke,
            traces,
            rho_milli,
        });
    }
}
