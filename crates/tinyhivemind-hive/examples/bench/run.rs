//! The host side of one episode: a journal, a roster, and the step loop.
//!
//! This is the whole contract a consuming host has to honour, and it is worth
//! reading as the shortest statement of it: fold, run the single turn the
//! library authorized, append that turn, commit the returned state, repeat.
//! The library never appends, never waits, and never calls back into the host.

use std::time::{Duration, Instant};

use tinyhivemind_hive::{
    Conversation, EpisodePolicy, EpisodeState, HiveStep, Sequence, SessionAuthor, SessionMessage,
    desk::{Desk, DeskSet, ResponderMode},
    project_for,
    roster::{Roster, RosterMember},
    step,
    trace::TopicId,
};

use crate::sim::{Room, SimAgent};

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
    /// The transcript, kept only for the single-episode trace view.
    pub(crate) journal: Vec<SessionMessage>,
    /// One line per turn, for the trace view.
    pub(crate) trace: Vec<String>,
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
        let next = u64::try_from(self.journal.len()).unwrap_or(u64::MAX).saturating_add(1);
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
    let ids = room.member_ids();
    let mut host = Host::new(&ids);
    host.operator(task);

    let mut agents: Vec<SimAgent> = room.agents.clone();
    let mut state = EpisodeState::opened(host.conversation(), host.watermark());
    let mut library_time = Duration::ZERO;
    let mut step_calls = 0_u32;
    let mut turns = 0_u32;
    let mut trace = Vec::new();

    loop {
        let started = Instant::now();
        let decision = {
            let roster = host.roster();
            let desks = host.desks();
            step(&state, &host.journal, &roster, &desks, policy)
        };
        library_time += started.elapsed();
        step_calls = step_calls.saturating_add(1);

        let decision = decision.map_err(|error| error.to_string())?;
        let (ending, decided) = match decision {
            HiveStep::Speak { turn } => {
                let turn = *turn;
                let visible = project_for(&turn, &host.journal);
                let Some(agent) = agents
                    .iter_mut()
                    .find(|agent| agent.id == turn.agent_id)
                else {
                    return Err(format!("no agent named {}", turn.agent_id));
                };
                let content = agent.speak(&turn, &visible);
                if keep_trace {
                    trace.push(format!(
                        "{:>10}  {:<10} {:<7} saw {:>2}  {content}",
                        turn.agent_id,
                        format!("{:?}", turn.reason).to_lowercase(),
                        format!("{:?}", turn.visibility).to_lowercase(),
                        visible.len(),
                    ));
                }
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

        let correct = decided.as_ref() == Some(&room.truth);
        return Ok(EpisodeReport {
            ending,
            decided,
            correct,
            turns,
            step_calls,
            library_time,
            journal: if keep_trace {
                host.journal.clone()
            } else {
                Vec::new()
            },
            trace,
        });
    }
}
