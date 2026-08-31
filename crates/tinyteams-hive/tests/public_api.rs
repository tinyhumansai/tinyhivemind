//! Public API regression tests for the hive crate.
//!
//! These import only through the published paths, so a change that moves an
//! item out of the root re-export surface fails here rather than in a host.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tinyteams_hive::{
    AgentThreshold, Bid, BidReason, ConsensusState, EpisodePolicy, EpisodeState, HiveStep,
    HiveTurn, Phase, QuorumPolicy, Salience, SalienceWeights, TRACE_CAP, TopicId, TopicStanding,
    Trace, TraceKind, Visibility, consensus, floor_holder, project_for, read, resolve, salience,
    standings, step,
};
// The runtime and the pure algebra arrive through this crate, so a host takes
// one dependency and the types it hands to `step` are the same types.
use tinyteams_hive::{
    Conversation, Sequence, SessionAuthor, SessionMessage,
    desk::{Desk, DeskSet, ResponderMode},
    roster::{Roster, RosterMember},
};

fn agent(id: &str) -> SessionAuthor {
    SessionAuthor::Agent {
        id: id.into(),
        label: id.into(),
    }
}

fn said(sequence: u64, id: &str, content: &str) -> SessionMessage {
    SessionMessage {
        sequence: Sequence(sequence),
        author: agent(id),
        content: content.into(),
    }
}

#[test]
fn root_exports_the_trace_grammar_and_its_cap() {
    let traces = resolve("!propose #stage ^1", None, &agent("planner"), Sequence(2));
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].kind, TraceKind::Propose);
    assert_eq!(traces[0].topic, Some(TopicId("stage".into())));
    assert!(traces[0].grounded());
    assert_eq!(TRACE_CAP, 16);

    let folded: Vec<Trace> = read(&[said(1, "planner", "!question")]);
    assert_eq!(folded.len(), 1);
}

#[test]
fn root_exports_the_salience_fold_and_its_defaults() {
    let trace = resolve("!commit #stage", None, &agent("planner"), Sequence(1))
        .pop()
        .expect("a trace");
    let score: Salience =
        salience(&trace, Sequence(1), &SalienceWeights::DEFAULT, 50).expect("scores");
    assert!(score.0 > 0);
    assert_eq!(SalienceWeights::DEFAULT.half_life, 20);
}

#[test]
fn root_exports_quorum_standings_and_consensus() {
    let transcript = [
        said(1, "planner", "!propose #stage"),
        said(2, "critic", "!support #stage ^1"),
    ];
    let policy = QuorumPolicy::DEFAULT;
    let standings: Vec<TopicStanding> =
        standings(&read(&transcript), Sequence(2), &policy).expect("folds");
    assert_eq!(
        consensus(&standings, &policy),
        ConsensusState::Quorum {
            topic: TopicId("stage".into()),
        },
    );
}

#[test]
fn root_exports_the_attention_market() {
    let bids = [
        Bid {
            agent_id: "planner".into(),
            urge: 10,
            reason: BidReason::Salience,
        },
        Bid {
            agent_id: "critic".into(),
            urge: 20,
            reason: BidReason::Dissent,
        },
    ];
    assert_eq!(floor_holder(&bids).expect("a winner").agent_id, "critic");
    assert_eq!(AgentThreshold::new("planner", 5).relevance(None), 50);
}

#[test]
fn root_exports_the_episode_state_machine() {
    let members = [RosterMember {
        id: "planner".into(),
        name: None,
    }];
    let desks = [Desk {
        id: "engineering".into(),
        name: "Engineering".into(),
        description: None,
        members: vec!["planner".into()],
        responder_mode: ResponderMode::Auto,
    }];
    let conversation = Conversation {
        desk_id: "engineering".into(),
        desk_name: "Engineering".into(),
        thread_root: None,
    };
    let state = EpisodeState::opened(conversation, Sequence(0));
    assert_eq!(state.phase, Phase::Deliberate);

    let decision = step(
        &state,
        &[said(1, "planner", "!propose #stage")],
        &Roster::new(&members, &[], &[]),
        &DeskSet::new(&desks, &[], &[], &[], &[]),
        &EpisodePolicy::DEFAULT,
    )
    .expect("steps");

    let HiveStep::Speak { turn } = decision else {
        panic!("expected exactly one turn")
    };
    let turn: HiveTurn = *turn;
    assert_eq!(turn.agent_id, "planner");
    assert_eq!(turn.next_state.spent, 1);
    assert_eq!(project_for(&turn, &[said(1, "planner", "x")]).len(), 1);
}

#[test]
fn a_blind_turn_withholds_a_peer_position_through_the_public_api() {
    let turn = HiveTurn {
        agent_id: "planner".into(),
        phase: Phase::Deliberate,
        visibility: Visibility::Blind,
        reason: BidReason::Salience,
        next_state: EpisodeState::opened(
            Conversation {
                desk_id: "engineering".into(),
                desk_name: "Engineering".into(),
                thread_root: None,
            },
            Sequence(0),
        ),
    };
    let transcript = [
        SessionMessage {
            sequence: Sequence(1),
            author: SessionAuthor::Operator,
            content: "the task".into(),
        },
        said(2, "planner", "my own position"),
        said(3, "critic", "a peer position"),
    ];
    assert_eq!(
        project_for(&turn, &transcript)
            .iter()
            .map(|message| message.sequence.0)
            .collect::<Vec<_>>(),
        [1, 2],
    );
}

#[test]
fn the_default_policy_bounds_every_episode() {
    // A finite budget is what makes termination a property rather than a hope.
    assert_eq!(EpisodePolicy::DEFAULT.turn_budget, 12);
    assert!(EpisodePolicy::DEFAULT.blind_round);
    assert_eq!(QuorumPolicy::DEFAULT.threshold, 2);
    assert!(QuorumPolicy::DEFAULT.require_grounded);
}
