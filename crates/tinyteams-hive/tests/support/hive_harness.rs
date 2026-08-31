//! A small in-memory host that drives one deliberation episode.
//!
//! The harness deliberately owns the journal and the model engines, exactly as
//! a consuming host must. The library under test contributes only the decision
//! about who speaks next and whether the room has finished — it never appends,
//! never waits, and never calls back into the host.

#![allow(dead_code)]

use tinyteams::{Conversation, SessionAuthor, SessionMessage, Sequence};
use tinyteams_hive::{
    EpisodePolicy, EpisodeState, HiveStep, HiveTurn, project_for,
    desk::{Desk, DeskSet, ResponderMode},
    quorum::TopicStanding,
    roster::{Roster, RosterMember},
    trace::TopicId,
};

/// A deterministic or model-backed participant.
pub(crate) trait HiveAgent {
    fn id(&self) -> &str;

    /// Produce the body of one turn, given exactly what this turn may see.
    fn speak(&mut self, turn: &HiveTurn, visible: &[&SessionMessage]) -> Result<String, String>;
}

/// Why an episode stopped.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Outcome {
    Converged {
        topic: TopicId,
        standing: TopicStanding,
    },
    Deadlocked {
        topics: Vec<TopicId>,
    },
    Exhausted {
        spent: u32,
    },
    Idle,
}

/// One recorded step of an episode, for assertions and for the example.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Step {
    pub(crate) agent_id: String,
    pub(crate) reason: tinyteams_hive::BidReason,
    pub(crate) visibility: tinyteams_hive::Visibility,
    pub(crate) phase: tinyteams_hive::Phase,
    pub(crate) saw: usize,
    pub(crate) content: String,
}

/// A host-owned append-only journal plus the episode loop over it.
pub(crate) struct HiveHarness {
    journal: Vec<SessionMessage>,
    members: Vec<RosterMember>,
    desks: Vec<Desk>,
    retired: Vec<String>,
    conversation: Conversation,
}

impl HiveHarness {
    pub(crate) fn new(desk_id: &str, desk_name: &str, member_ids: &[&str]) -> Self {
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
                id: desk_id.to_owned(),
                name: desk_name.to_owned(),
                description: None,
                members: member_ids.iter().map(|id| (*id).to_owned()).collect(),
                responder_mode: ResponderMode::Auto,
            }],
            retired: Vec::new(),
            conversation: Conversation {
                desk_id: desk_id.to_owned(),
                desk_name: desk_name.to_owned(),
                thread_root: None,
            },
        }
    }

    pub(crate) fn conversation(&self) -> Conversation {
        self.conversation.clone()
    }

    pub(crate) fn journal(&self) -> &[SessionMessage] {
        &self.journal
    }

    pub(crate) fn watermark(&self) -> Sequence {
        self.journal
            .last()
            .map_or(Sequence(0), |message| message.sequence)
    }

    fn append(&mut self, author: SessionAuthor, content: impl Into<String>) -> Sequence {
        let sequence = Sequence(self.journal.len() as u64 + 1);
        self.journal.push(SessionMessage {
            sequence,
            author,
            content: content.into(),
        });
        sequence
    }

    pub(crate) fn operator(&mut self, content: &str) -> Sequence {
        self.append(SessionAuthor::Operator, content)
    }

    pub(crate) fn agent(&mut self, id: &str, content: &str) -> Sequence {
        self.append(
            SessionAuthor::Agent {
                id: id.to_owned(),
                label: id.to_owned(),
            },
            content,
        )
    }

    /// Run an episode to termination, appending each authorized turn.
    ///
    /// This is the whole host contract: fold, run the one turn the library
    /// authorized, append it, commit the returned state, repeat.
    pub(crate) fn run(
        &mut self,
        state: EpisodeState,
        policy: &EpisodePolicy,
        agents: &mut [&mut dyn HiveAgent],
    ) -> Result<(Outcome, Vec<Step>), String> {
        let mut state = state;
        let mut steps = Vec::new();
        loop {
            let members = Roster::new(&self.members, &[], &self.retired);
            let desks = DeskSet::new(&self.desks, &[], &[], &[], &self.retired);
            let decision = tinyteams_hive::step(&state, &self.journal, &members, &desks, policy)
                .map_err(|error| error.to_string())?;

            let turn = match decision {
                HiveStep::Speak { turn } => *turn,
                HiveStep::Converged { topic, standing } => {
                    return Ok((
                        Outcome::Converged {
                            topic,
                            standing: *standing,
                        },
                        steps,
                    ));
                }
                HiveStep::Deadlocked { topics } => {
                    return Ok((Outcome::Deadlocked { topics }, steps));
                }
                HiveStep::Exhausted { spent } => {
                    return Ok((Outcome::Exhausted { spent }, steps));
                }
                HiveStep::Idle => return Ok((Outcome::Idle, steps)),
            };

            let visible = project_for(&turn, &self.journal);
            let agent = agents
                .iter_mut()
                .find(|agent| agent.id() == turn.agent_id)
                .ok_or_else(|| format!("no agent named {}", turn.agent_id))?;
            let content = agent.speak(&turn, &visible)?;

            steps.push(Step {
                agent_id: turn.agent_id.clone(),
                reason: turn.reason,
                visibility: turn.visibility,
                phase: turn.phase,
                saw: visible.len(),
                content: content.clone(),
            });

            // The turn is durably appended before its state is committed, which
            // is the ordering the library's `next_state` contract requires.
            self.agent(&turn.agent_id, &content);
            state = turn.next_state;
        }
    }
}
