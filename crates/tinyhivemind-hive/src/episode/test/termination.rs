//! The single-turn invariant and termination: every step either authorizes
//! exactly one turn or returns a terminal outcome, `spent` strictly advances
//! and the budget check runs before it can overflow, so the loop cannot run
//! past its budget.

use super::super::*;
use super::support::{MEMBERS, Room, converging, operator, run, said, sequential, spoke, state};
use crate::salience::SalienceWeights;

#[test]
fn a_speaking_step_authorizes_exactly_one_turn() {
    let room = Room::new();
    let (turn, next) = spoke(run(&room, &state(), &converging(), &sequential()));
    assert!(MEMBERS.contains(&turn.agent_id.as_str()));
    assert_eq!(next.spent, 1);
}

#[test]
fn a_spent_budget_is_exhausted_and_authorizes_no_turn() {
    let room = Room::new();
    let policy = EpisodePolicy {
        turn_budget: 4,
        ..sequential()
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
        ..sequential()
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
        ..sequential()
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
        let (_turn, next) = spoke(run(&room, &state, &transcript, &policy));
        assert_eq!(next.spent, expected);
        state = next;
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
        run(&room, &state, &converging(), &sequential()),
        HiveStep::Idle,
    );
}

#[test]
fn the_budget_check_bounds_the_spend_before_it_can_overflow() {
    let room = Room::new();
    let policy = EpisodePolicy {
        turn_budget: u32::MAX,
        ..sequential()
    };
    // One below the ceiling still advances, landing exactly on it...
    let brimming = EpisodeState {
        spent: u32::MAX - 1,
        ..state()
    };
    let (_turn, next) = spoke(run(&room, &brimming, &converging(), &policy));
    assert_eq!(next.spent, u32::MAX);

    // ...and at the ceiling the budget check fires first, so the addition is
    // never reached. That is why there is no overflow error to return.
    assert!(matches!(
        run(&room, &next, &converging(), &policy),
        HiveStep::Exhausted {
            spent: u32::MAX,
            ..
        },
    ));
}

#[test]
fn exhaustion_reports_the_standings_the_budget_bought() {
    let room = Room::new();
    let policy = EpisodePolicy {
        turn_budget: 3,
        ..sequential()
    };
    let spent = EpisodeState {
        spent: 3,
        ..state()
    };
    let HiveStep::Exhausted { standings, .. } = run(&room, &spent, &converging(), &policy) else {
        panic!("expected an exhausted episode")
    };
    // `converging()` puts one proposal on the floor with two grounded
    // supporters, and the room ran out of budget holding exactly that.
    let stage = standings
        .iter()
        .find(|standing| standing.topic.to_string() == "stage")
        .expect("the advocated topic is reported");
    assert_eq!(stage.supporters, vec!["planner", "critic"]);
}

#[test]
fn exhaustion_after_a_silent_episode_reports_no_standings() {
    let room = Room::new();
    let policy = EpisodePolicy {
        turn_budget: 3,
        ..sequential()
    };
    let spent = EpisodeState {
        spent: 3,
        ..state()
    };
    // Three turns spent, nothing advocated: the distinguishing report, and the
    // one a federation whose desks spend their budget asking each other
    // questions actually produces.
    let quiet = [
        operator(1, "Decide how to roll this out."),
        said(2, "planner", "Looking into it."),
        said(3, "critic", "Same."),
    ];
    let HiveStep::Exhausted { standings, .. } = run(&room, &spent, &quiet, &policy) else {
        panic!("expected an exhausted episode")
    };
    assert!(standings.is_empty());
}

#[test]
fn exhaustion_reports_a_room_that_never_saw_itself() {
    let room = Room::new();
    // Two turns for three members: under a blind opening round the third
    // member never authors, so visibility never lifts and the room takes its
    // whole budget without once seeing a peer's position.
    let policy = EpisodePolicy {
        turn_budget: 2,
        ..sequential()
    };
    let spent = EpisodeState {
        spent: 2,
        ..state()
    };
    let HiveStep::Exhausted { visibility, .. } = run(&room, &spent, &converging(), &policy) else {
        panic!("expected an exhausted episode")
    };
    assert_eq!(visibility, Visibility::Blind);

    // The same room with a budget that clears the roster does see itself.
    let heard = [
        said(1, "planner", "!propose #stage"),
        said(2, "critic", "!support #stage ^1"),
        said(3, "scout", "!support #stage ^1"),
    ];
    let policy = EpisodePolicy {
        turn_budget: 9,
        ..sequential()
    };
    let spent = EpisodeState {
        spent: 9,
        ..state()
    };
    let HiveStep::Exhausted { visibility, .. } = run(&room, &spent, &heard, &policy) else {
        panic!("expected an exhausted episode")
    };
    assert_eq!(visibility, Visibility::Full);
}

#[test]
fn a_policy_for_a_room_scales_every_absolute_bound_with_it() {
    // The three numbers `DEFAULT` states absolutely, against a desk of 256.
    let policy = EpisodePolicy::for_room(256);
    assert_eq!(policy.turn_budget, 768);
    assert_eq!(policy.quorum.threshold, 129);
    assert_eq!(policy.quorum.window, 768);
    assert_eq!(policy.weights.half_life, 256);
    // The blind round is the whole room at once; a revealed one stays at one,
    // which is the shipping default's own rule rather than a new opinion.
    assert_eq!(policy.round_width, 256);
    assert_eq!(policy.revealed_width, 1);

    // A budget above the roster, so the blind round can always complete —
    // which is the failure `DEFAULT` produces on any desk above a dozen.
    assert!(policy.turn_budget > 256);

    // A threshold above half the desk, so no two *disjoint* supporter sets can
    // both carry — one member backing both topics still can, which
    // `deadlock::a_majority_threshold_does_not_make_deadlock_unreachable`
    // pins — and below the whole of it, so one grounded objection cannot make
    // quorum unreachable.
    assert!(policy.quorum.threshold > 256 / 2);
    assert!(policy.quorum.threshold < 256);
}

#[test]
fn a_policy_for_a_small_room_keeps_the_defaults_it_should() {
    // A two-member desk still needs two supporters, not one and a half, and a
    // room smaller than the default half-life keeps the default.
    let policy = EpisodePolicy::for_room(2);
    assert_eq!(policy.quorum.threshold, 2);
    assert_eq!(policy.weights.half_life, SalienceWeights::DEFAULT.half_life);
    // The floor keeps a tiny room able to open, support and record.
    assert_eq!(policy.turn_budget, 6);
    assert_eq!(policy.round_width, 2);

    // A round is never zero-width: a policy that authorizes no turns would
    // make the episode unable to advance rather than merely narrow.
    assert_eq!(EpisodePolicy::for_room(0).round_width, 1);

    // A one-member desk cannot reach a threshold of two, and the constructor
    // does not pretend otherwise: `members - 1` is zero, so the floor of two
    // stands and the room will exhaust. That is a true report of a desk too
    // small to hold a quorum, not a policy that hides it.
    assert_eq!(EpisodePolicy::for_room(1).quorum.threshold, 2);
}
