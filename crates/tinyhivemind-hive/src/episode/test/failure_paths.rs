//! Validation failure paths: a malformed roster, desk snapshot, or policy is
//! rejected before a turn is authorized, and a threshold naming someone who
//! is not an active desk member is rejected the same way a retired member is
//! quietly dropped from bidding and from the carried thresholds.

use super::super::*;
use super::support::{spoke, Room, converging, desks, member, run, sequential, speaking, state};
use tinyhivemind::{Conversation, Sequence};

#[test]
fn a_malformed_roster_or_desk_snapshot_is_rejected() {
    let mut room = Room::new();
    room.members.push(member("planner"));
    let error = step(
        &state(),
        &converging(),
        &room.roster(),
        &room.desk_set(),
        &sequential(),
    )
    .expect_err("duplicate roster member");
    assert_eq!(error.to_string(), "duplicate roster member id `planner`");

    let mut room = Room::new();
    room.desks.push(desks().remove(0));
    let error = step(
        &state(),
        &converging(),
        &room.roster(),
        &room.desk_set(),
        &sequential(),
    )
    .expect_err("duplicate desk");
    assert_eq!(error.to_string(), "duplicate desk id `engineering`");
}

#[test]
fn an_unknown_desk_is_rejected() {
    let room = Room::new();
    let elsewhere = EpisodeState::opened(
        Conversation {
            desk_id: "design".into(),
            desk_name: "Design".into(),
            thread_root: None,
        },
        Sequence(0),
    );
    let error = step(
        &elsewhere,
        &converging(),
        &room.roster(),
        &room.desk_set(),
        &sequential(),
    )
    .expect_err("unknown desk");
    assert_eq!(error.to_string(), "unknown desk `design`");
}

#[test]
fn a_threshold_naming_a_non_member_is_rejected() {
    let room = Room::new();
    let state = EpisodeState {
        thresholds: vec![AgentThreshold::new("stranger", 0)],
        ..state()
    };
    let error = step(
        &state,
        &converging(),
        &room.roster(),
        &room.desk_set(),
        &sequential(),
    )
    .expect_err("unknown threshold member");
    assert_eq!(
        error.to_string(),
        "threshold `stranger` is not an active member of desk `engineering`",
    );
}

#[test]
fn a_retired_member_neither_bids_nor_holds_a_threshold() {
    let mut room = Room::new();
    room.retired = vec!["scout".into()];
    let (turn, next) = spoke(run(&room, &state(), &converging(), &sequential()));
    assert_ne!(turn.agent_id, "scout");
    assert!(
        next.thresholds
            .iter()
            .all(|held| held.agent_id != "scout"),
    );
}

#[test]
fn a_malformed_policy_surfaces_from_the_quorum_fold() {
    let room = Room::new();
    let policy = EpisodePolicy {
        quorum: crate::quorum::QuorumPolicy {
            threshold: 0,
            ..crate::quorum::QuorumPolicy::DEFAULT
        },
        ..sequential()
    };
    let error = step(
        &state(),
        &converging(),
        &room.roster(),
        &room.desk_set(),
        &policy,
    )
    .expect_err("zero threshold");
    assert_eq!(error.to_string(), "quorum threshold must not be zero");
}
