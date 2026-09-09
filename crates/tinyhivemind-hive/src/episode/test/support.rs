//! Shared fixtures for the episode test suite: a three-member `Room`,
//! transcript builders for a message authored, an aside, and an operator
//! notice, and the `run`/`speaking` helpers most tests drive `step` through.

use super::super::*;

use tinyhivemind::aside::Audience;
use tinyhivemind::{
    Conversation, Sequence,
    desk::{Desk, DeskSet, ResponderMode},
    roster::{Roster, RosterMember},
};

pub(super) const MEMBERS: [&str; 3] = ["planner", "critic", "scout"];

pub(super) fn member(id: &str) -> RosterMember {
    RosterMember {
        id: id.into(),
        name: Some(id.into()),
    }
}

pub(super) fn roster_members() -> Vec<RosterMember> {
    MEMBERS.iter().map(|id| member(id)).collect()
}

pub(super) fn desks() -> Vec<Desk> {
    vec![Desk {
        id: "engineering".into(),
        name: "Engineering".into(),
        description: None,
        members: MEMBERS.iter().map(|id| (*id).to_owned()).collect(),
        responder_mode: ResponderMode::Auto,
    }]
}

pub(super) fn conversation() -> Conversation {
    Conversation {
        desk_id: "engineering".into(),
        desk_name: "Engineering".into(),
        thread_root: None,
    }
}

/// A desk-visible message authored by an agent.
pub(super) fn said(sequence: u64, author: &str, content: &str) -> SessionMessage {
    SessionMessage {
        sequence: Sequence(sequence),
        author: SessionAuthor::Agent {
            id: author.into(),
            label: author.into(),
        },
        content: content.into(),
        audience: Audience::Desk,
        elided: None,
    }
}

/// A desk-visible message authored by the operator.
pub(super) fn operator(sequence: u64, content: &str) -> SessionMessage {
    SessionMessage {
        sequence: Sequence(sequence),
        author: SessionAuthor::Operator,
        content: content.into(),
        audience: Audience::Desk,
        elided: None,
    }
}

/// One row addressed privately to `members`.
pub(super) fn aside(
    sequence: u64,
    author: &str,
    members: &[&str],
    content: &str,
) -> SessionMessage {
    SessionMessage {
        audience: Audience::Aside {
            members: members.iter().map(|member| (*member).to_owned()).collect(),
        },
        ..said(sequence, author, content)
    }
}

/// A desk of three active members, plus who has retired from it.
pub(super) struct Room {
    pub(super) members: Vec<RosterMember>,
    pub(super) desks: Vec<Desk>,
    pub(super) retired: Vec<String>,
}

impl Room {
    pub(super) fn new() -> Self {
        Self {
            members: roster_members(),
            desks: desks(),
            retired: Vec::new(),
        }
    }

    pub(super) fn roster(&self) -> Roster<'_> {
        Roster::new(&self.members, &[], &self.retired)
    }

    pub(super) fn desk_set(&self) -> DeskSet<'_> {
        DeskSet::new(&self.desks, &[], &[], &[], &self.retired)
    }
}

/// A freshly opened episode state, at the watermark of an empty transcript.
pub(super) fn state() -> EpisodeState {
    EpisodeState::opened(conversation(), Sequence(0))
}

/// Run `step` for a room, unwrapping the result: every fixture here is valid.
pub(super) fn run(
    room: &Room,
    state: &EpisodeState,
    transcript: &[SessionMessage],
    policy: &EpisodePolicy,
) -> HiveStep {
    step(state, transcript, &room.roster(), &room.desk_set(), policy).expect("steps")
}

/// The sequential episode: a round of one, which is what most of these
/// fixtures assert the dynamics of.
///
/// `EpisodePolicy::DEFAULT` runs wider rounds, because a seat is an async
/// session. Width one is still a supported configuration rather than a
/// deprecated one, and the tests that pin single-turn behaviour say so by
/// asking for it rather than by inheriting it.
pub(super) fn sequential() -> EpisodePolicy {
    EpisodePolicy {
        round_width: 1,
        revealed_width: 1,
        ..EpisodePolicy::DEFAULT
    }
}

/// Unwrap a `HiveStep::Speak` of exactly one turn, panicking otherwise.
pub(super) fn speaking(step: HiveStep) -> HiveTurn {
    spoke(step).0
}

/// The same, with the state the round commits — which belongs to the round
/// rather than to the turn, so a fixture that needs both asks for both.
pub(super) fn spoke(step: HiveStep) -> (HiveTurn, EpisodeState) {
    let (mut turns, next_state) = round(step);
    assert_eq!(turns.len(), 1, "expected a round of one, got {turns:?}");
    (turns.remove(0), next_state)
}

/// Unwrap a `HiveStep::Speak` into its round and the state it commits.
pub(super) fn round(step: HiveStep) -> (Vec<HiveTurn>, EpisodeState) {
    let HiveStep::Speak { turns, next_state } = step else {
        panic!("expected a round, got {step:?}")
    };
    (turns, *next_state)
}

/// A room with two grounded supporters behind one proposal.
pub(super) fn converging() -> Vec<SessionMessage> {
    vec![
        operator(1, "Decide how to roll this out."),
        said(2, "planner", "!propose #stage Stage the rollout."),
        said(3, "critic", "!support #stage ^2 Bounds the blast radius."),
    ]
}

/// A room with two proposals each holding two grounded supporters.
pub(super) fn deadlocked() -> Vec<SessionMessage> {
    vec![
        said(1, "planner", "!propose #stage"),
        said(2, "scout", "!propose #ship"),
        said(3, "critic", "!support #stage ^1"),
        said(4, "planner", "!support #ship ^2"),
    ]
}
