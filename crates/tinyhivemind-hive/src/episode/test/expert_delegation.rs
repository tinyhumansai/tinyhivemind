//! Expert delegation: `policy.directory` and `policy.defer_cap` are both
//! required-but-nullable on the wire and both `None` by default, and turning
//! them on routes the floor to the member the transcript says holds the
//! contested topic rather than to ordinary salience.

use super::super::*;
use super::support::{Room, converging, deadlocked, operator, run, said, sequential, speaking, state};
use crate::{attention::BidReason, directory::DirectoryPolicy};

/// A room arguing an ungrounded decoy while the scout's fact goes unheard.
///
/// The critic's opinion cites nothing, so under `require_grounded` it never
/// joins the supporter set and `#retries` stays one short of carrying. The
/// scout has put the fact that settles it on the floor and taken no position.
fn unheard() -> Vec<SessionMessage> {
    vec![
        operator(1, "Why did latency spike?"),
        said(2, "planner", "!propose #retries A retry storm explains it."),
        said(3, "critic", "!support #retries The graph agrees."),
        said(
            4,
            "scout",
            "!evidence #retries The retry flag has been off for a week.",
        ),
    ]
}

fn delegating() -> EpisodePolicy {
    EpisodePolicy {
        directory: Some(DirectoryPolicy {
            window: 100,
            ..DirectoryPolicy::DEFAULT
        }),
        ..sequential()
    }
}

#[test]
fn a_policy_without_the_directory_key_is_rejected() {
    let mut value = serde_json::to_value(EpisodePolicy::DEFAULT).expect("serializes");
    let object = value.as_object_mut().expect("an object");
    object.remove("directory");
    assert!(
        serde_json::from_value::<EpisodePolicy>(value).is_err(),
        "an absent key must not silently mean off",
    );
}

#[test]
fn a_policy_without_the_defer_cap_key_is_rejected() {
    let mut value = serde_json::to_value(EpisodePolicy::DEFAULT).expect("serializes");
    let object = value.as_object_mut().expect("an object");
    object.remove("defer_cap");
    assert!(serde_json::from_value::<EpisodePolicy>(value).is_err());
}

#[test]
fn the_default_policy_leaves_expert_delegation_off() {
    assert_eq!(EpisodePolicy::DEFAULT.directory, None);
    assert_eq!(EpisodePolicy::DEFAULT.defer_cap, None);
}

#[test]
fn a_zero_defer_cap_is_rejected() {
    let room = Room::new();
    let policy = EpisodePolicy {
        defer_cap: Some(0),
        ..sequential()
    };
    let error = step(
        &state(),
        &converging(),
        &room.roster(),
        &room.desk_set(),
        &policy,
    )
    .expect_err("zero defer cap");
    assert_eq!(error.to_string(), "defer cap must not be zero");
}

#[test]
fn a_zero_directory_half_life_is_rejected_even_when_the_budget_is_spent() {
    // The budget check alone would return `Exhausted` here without ever
    // reaching the directory fold, which is exactly why the policy has to be
    // validated ahead of every terminal return rather than only inside the
    // fold: a malformed `DirectoryPolicy` must fail the same way on every
    // step, not just the ones that get far enough to call `directory`.
    let room = Room::new();
    let policy = EpisodePolicy {
        turn_budget: 0,
        directory: Some(DirectoryPolicy {
            half_life: 0,
            ..DirectoryPolicy::DEFAULT
        }),
        ..sequential()
    };
    let error = step(
        &state(),
        &converging(),
        &room.roster(),
        &room.desk_set(),
        &policy,
    )
    .expect_err("zero directory half-life");
    assert_eq!(error.to_string(), "directory half life must not be zero");
}

#[test]
fn an_episode_with_a_directory_gives_the_floor_to_the_fact_holder() {
    let room = Room::new();
    let turn = speaking(run(&room, &state(), &unheard(), &delegating()));
    assert_eq!(turn.agent_id, "scout");
    assert_eq!(turn.reason, BidReason::Knows);
}

#[test]
fn an_episode_without_a_directory_reaches_the_same_decision_as_before() {
    let room = Room::new();
    // Every transcript the rest of this module exercises, stepped with the
    // shipping default. The expected step is pinned as a literal rather than
    // compared against a second `DEFAULT` run, which would only assert that
    // the default agrees with itself: a change to what the default decides has
    // to fail here.
    let converged = speaking(run(&room, &state(), &converging(), &sequential()));
    assert_eq!(converged.agent_id, "planner");
    assert_eq!(converged.reason, BidReason::Addressed);
    assert_eq!(converged.phase, Phase::Commit);

    assert_eq!(
        run(&room, &state(), &deadlocked(), &sequential()),
        HiveStep::Deadlocked {
            topics: vec!["stage".into(), "ship".into()],
        },
    );

    // The hidden profile the delegating policy solves: without a directory the
    // floor goes to the proposer on ordinary salience, and the scout's
    // uncited fact stays uncited.
    let unrouted = speaking(run(&room, &state(), &unheard(), &sequential()));
    assert_eq!(unrouted.agent_id, "planner");
    assert_eq!(unrouted.reason, BidReason::Salience);
    assert_eq!(unrouted.phase, Phase::Deliberate);

    // And turning the two knobs off explicitly reaches the same three steps,
    // so `None` really is the shipping default rather than a second mode.
    for transcript in [converging(), deadlocked(), unheard()] {
        let off = EpisodePolicy {
            directory: None,
            defer_cap: None,
            ..sequential()
        };
        assert_eq!(
            run(&room, &state(), &transcript, &sequential()),
            run(&room, &state(), &transcript, &off),
        );
    }
}

#[test]
fn a_defer_chain_terminates_at_the_defer_cap() {
    let room = Room::new();
    let policy = EpisodePolicy {
        defer_cap: Some(2),
        ..delegating()
    };
    let transcript = vec![
        operator(1, "Why did latency spike?"),
        said(2, "planner", "!propose #retries A retry storm explains it."),
        said(3, "scout", "!evidence #pool In-flight requests sit at 24."),
        said(4, "planner", "!defer #pool Not my area."),
        said(5, "critic", "!defer #pool Nor mine."),
    ];
    // Two live deferrals reach the cap, so `#pool` stops being promoted and
    // the room goes back to the standing it has.
    let turn = speaking(run(&room, &state(), &transcript, &policy));
    assert_ne!(turn.reason, BidReason::Knows);

    // One fewer, and the deferral still routes to the holder.
    let turn = speaking(run(&room, &state(), &transcript[..4], &policy));
    assert_eq!(turn.agent_id, "scout");
    assert_eq!(turn.reason, BidReason::Knows);
}

#[test]
fn a_defer_chain_terminates_at_the_turn_budget_without_a_cap() {
    let room = Room::new();
    let policy = EpisodePolicy {
        turn_budget: 3,
        defer_cap: None,
        ..delegating()
    };
    let mut transcript = vec![operator(1, "Why did latency spike?")];
    for sequence in 2..12 {
        transcript.push(said(sequence, "planner", "!defer #pool Not my area."));
    }
    // Nothing caps the chain, so the budget does. It is finite, so it must.
    let spent = EpisodeState {
        spent: 3,
        ..state()
    };
    assert_eq!(
        run(&room, &spent, &transcript, &policy),
        HiveStep::Exhausted { spent: 3 },
    );
}
