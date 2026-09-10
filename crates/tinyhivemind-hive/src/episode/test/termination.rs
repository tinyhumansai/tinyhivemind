//! The single-turn invariant and termination: every step either authorizes
//! exactly one turn or returns a terminal outcome, `spent` strictly advances
//! and the budget check runs before it can overflow, so the loop cannot run
//! past its budget.

use super::super::*;
use super::support::{MEMBERS, Room, converging, operator, run, said, speaking, state};

#[test]
fn a_speaking_step_authorizes_exactly_one_turn() {
    let room = Room::new();
    let turn = speaking(run(&room, &state(), &converging(), &EpisodePolicy::DEFAULT));
    assert!(MEMBERS.contains(&turn.agent_id.as_str()));
    assert_eq!(turn.next_state.spent, 1);
}

#[test]
fn a_spent_budget_is_exhausted_and_authorizes_no_turn() {
    let room = Room::new();
    let policy = EpisodePolicy {
        turn_budget: 4,
        ..EpisodePolicy::DEFAULT
    };
    let spent = EpisodeState {
        spent: 4,
        ..state()
    };
    assert!(matches!(
        run(&room, &spent, &converging(), &policy),
        HiveStep::Exhausted { spent: 4, .. },
    ));
}

#[test]
fn a_zero_budget_never_authorizes_a_first_turn() {
    let room = Room::new();
    let policy = EpisodePolicy {
        turn_budget: 0,
        ..EpisodePolicy::DEFAULT
    };
    assert!(matches!(
        run(&room, &state(), &converging(), &policy),
        HiveStep::Exhausted { spent: 0, .. },
    ));
}

#[test]
fn an_episode_terminates_within_its_budget() {
    let room = Room::new();
    let policy = EpisodePolicy {
        turn_budget: 5,
        ..EpisodePolicy::DEFAULT
    };
    let mut state = state();
    // One proposal only: below quorum, so the room keeps deliberating and the
    // budget is what stops it rather than a decision.
    let transcript = [
        operator(1, "Decide how to roll this out."),
        said(2, "planner", "!propose #stage Stage the rollout."),
    ];
    // Every step either terminates or strictly advances the spend, so the loop
    // cannot run past the budget.
    for expected in 1..=policy.turn_budget {
        let turn = speaking(run(&room, &state, &transcript, &policy));
        assert_eq!(turn.next_state.spent, expected);
        state = turn.next_state;
    }
    assert!(matches!(
        run(&room, &state, &transcript, &policy),
        HiveStep::Exhausted { spent: 5, .. },
    ));
}

#[test]
fn nobody_speaks_when_every_threshold_is_unreachable() {
    let room = Room::new();
    let state = EpisodeState {
        thresholds: MEMBERS
            .iter()
            .map(|id| AgentThreshold::new(*id, i64::MAX))
            .collect(),
        ..state()
    };
    assert_eq!(
        run(&room, &state, &converging(), &EpisodePolicy::DEFAULT),
        HiveStep::Idle,
    );
}

#[test]
fn the_budget_check_bounds_the_spend_before_it_can_overflow() {
    let room = Room::new();
    let policy = EpisodePolicy {
        turn_budget: u32::MAX,
        ..EpisodePolicy::DEFAULT
    };
    // One below the ceiling still advances, landing exactly on it...
    let brimming = EpisodeState {
        spent: u32::MAX - 1,
        ..state()
    };
    let turn = speaking(run(&room, &brimming, &converging(), &policy));
    assert_eq!(turn.next_state.spent, u32::MAX);

    // ...and at the ceiling the budget check fires first, so the addition is
    // never reached. That is why there is no overflow error to return.
    assert!(matches!(
        run(&room, &turn.next_state, &converging(), &policy),
        HiveStep::Exhausted { spent: u32::MAX, .. },
    ));
}
