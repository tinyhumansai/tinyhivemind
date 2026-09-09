//! Tests for the aggregation the tables are folded out of.
//!
//! The statistics themselves are checked by `--stats-check`, which runs
//! `wilson`, `paired_bootstrap` and `spearman_milli` against properties they
//! are defined to have. What is checked here is the fold: that merging two
//! samples is the same thing as folding one, which is what lets the per-room
//! loops run across cores without moving a number.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use super::Aggregate;
use crate::TASK;
use crate::arms;
use crate::policy::tuned_policy;
use crate::rng::mix;
use crate::run::run_episode;
use crate::sim::{Expertise, Room};

/// Rooms folded by both paths. Enough that the halves differ from each other
/// on every counter worth differing on.
const ROOMS: u32 = 24;

/// Zero the two measured-time fields so two folds of the same rooms can be
/// compared for equality.
///
/// `library_time` and `step_time` are wall-clock readings taken around real
/// calls, so two folds of the *same* episodes disagree on them by whatever the
/// machine was doing at the time. They are the only fields in [`Aggregate`]
/// that are not a function of the rooms, and that is worth knowing rather than
/// just working around: it is also why `ns/step` and `episodes/s` are the two
/// columns that legitimately move under `--jobs`, while every other number in
/// the table must not.
fn without_measured_time(aggregate: &Aggregate) -> Aggregate {
    let mut copy = aggregate.clone();
    copy.library_time = Duration::ZERO;
    copy.step_time = Duration::ZERO;
    copy
}

/// Generate the sample both paths fold.
///
/// A hidden profile rather than uniform noise, because it is the shape that
/// populates `expert_of`, `fact_deposited`, `fact_turns` and `routed_of` — a
/// uniform room names no expert, leaves those at zero, and would let a merge
/// that dropped one of them pass.
fn sample() -> Vec<Room> {
    (0..ROOMS)
        .map(|index| {
            Room::generate_with(
                mix(7, u64::from(index)),
                5,
                4,
                50,
                Expertise::HiddenProfile,
                true,
            )
        })
        .collect()
}

#[test]
fn merging_matches_folding_in_one_pass() {
    let rooms = sample();
    let policy = tuned_policy(5);

    let mut whole = Aggregate::default();
    for room in &rooms {
        whole.add(&run_episode(room, &policy, TASK, false).expect("the room is well formed"));
    }

    let (left, right) = rooms.split_at(rooms.len() / 2);
    let mut first = Aggregate::default();
    for room in left {
        first.add(&run_episode(room, &policy, TASK, false).expect("the room is well formed"));
    }
    let mut second = Aggregate::default();
    for room in right {
        second.add(&run_episode(room, &policy, TASK, false).expect("the room is well formed"));
    }
    first.merge(&second);

    // Compared whole rather than field by field on purpose: a counter added to
    // `Aggregate` later and folded by `add` but forgotten by `merge` fails
    // here without anybody remembering to extend this test.
    assert_eq!(
        without_measured_time(&first),
        without_measured_time(&whole),
        "merging two halves must equal folding the sample in one pass"
    );
}

#[test]
fn merging_arm_reports_matches_folding_in_one_pass() {
    // `add_arm` populates a different set of counters from `add` — the routing
    // pair above all — so the control arms need their own pass over the same
    // question.
    let rooms = sample();

    let mut whole = Aggregate::default();
    for (index, room) in rooms.iter().enumerate() {
        let seed = mix(7, u64::try_from(index).unwrap_or(0));
        whole.add_arm(&arms::run_ladder(room, seed).expect("the room is well formed"));
    }

    let mut first = Aggregate::default();
    let mut second = Aggregate::default();
    for (index, room) in rooms.iter().enumerate() {
        let seed = mix(7, u64::try_from(index).unwrap_or(0));
        let report = arms::run_ladder(room, seed).expect("the room is well formed");
        if index < rooms.len() / 2 {
            first.add_arm(&report);
        } else {
            second.add_arm(&report);
        }
    }
    first.merge(&second);

    assert_eq!(without_measured_time(&first), without_measured_time(&whole));
}

#[test]
fn merging_appends_the_paired_sample_rather_than_combining_it() {
    // The ordering guarantee stated as its own case. `paired_bootstrap`
    // resamples two arms index-for-index because they decided the same rooms,
    // so the flags have to arrive in room order and keep their length.
    let mut first = Aggregate::default();
    first.correct_flags = vec![true, false];
    let mut second = Aggregate::default();
    second.correct_flags = vec![false, true, true];

    first.merge(&second);

    assert_eq!(first.correct_flags, vec![true, false, false, true, true]);
}

#[test]
fn merging_an_empty_sample_changes_nothing() {
    let rooms = sample();
    let policy = tuned_policy(5);
    let mut folded = Aggregate::default();
    for room in &rooms {
        folded.add(&run_episode(room, &policy, TASK, false).expect("the room is well formed"));
    }
    let before = folded.clone();

    folded.merge(&Aggregate::default());

    assert_eq!(folded, before);
    assert_eq!(
        folded.library_time, before.library_time,
        "an empty sample adds no time either"
    );
}
