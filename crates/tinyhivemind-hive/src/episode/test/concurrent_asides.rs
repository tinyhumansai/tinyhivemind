//! Concurrent asides: a host may append an aside authored by the turn-holder
//! in the same turn as that member's desk-visible move, at no cost to the
//! room, but the invariant has a limit — the sequence a private row consumes
//! can still shift a later desk row across a quorum window.

use super::super::*;
use super::support::{spoke, Room, aside, operator, run, said, sequential, state};
use crate::quorum::QuorumPolicy;

#[test]
fn an_aside_riding_along_with_a_turn_costs_the_room_nothing() {
    // The concurrent-aside contract, and the property the whole of it rests
    // on: a host may append one aside row authored by the turn-holder in the
    // same turn as that member's desk-visible move, and the episode cannot
    // tell. `live_traces` drops a non-desk row before it can reach the
    // standings, the sequence they fold at, or the floor, and `spent`
    // counts turns rather than rows — so the two transcripts must step
    // identically, down to the state each turn commits.
    //
    // This is why a private exchange need not be charged a floor turn. See
    // ADR 0011.
    let room = Room::new();
    let policy = sequential();
    let plain = vec![
        operator(1, "Pick one."),
        said(2, "planner", "!propose #stage"),
        said(4, "critic", "!support #stage ^2"),
    ];
    let mut concurrent = plain.clone();
    concurrent.push(aside(
        3,
        "planner",
        &["critic"],
        "!aside @critic #stage What do you make of this one?",
    ));
    concurrent.push(aside(
        5,
        "critic",
        &["planner"],
        "!aside @planner #stage My own reading is 60.",
    ));
    concurrent.sort_by_key(|message| message.sequence);

    assert_eq!(
        run(&room, &state(), &plain, &policy),
        run(&room, &state(), &concurrent, &policy),
    );
}

#[test]
fn a_room_that_has_only_said_things_privately_has_not_started() {
    // The other direction, and the reason a concurrent aside cannot outrun
    // the room: an aside is not a turn and never becomes one. A transcript
    // of nothing but private rows leaves the episode exactly where it opened
    // — nothing proposed, nothing supported, no budget spent — so however
    // many asides ride along, the room still decides at the pace of its
    // floor.
    let room = Room::new();
    let policy = sequential();
    let private = vec![
        operator(1, "Pick one."),
        aside(2, "planner", &["critic"], "!propose #stage Quietly."),
        aside(3, "critic", &["planner"], "!support #stage ^2 Quietly."),
    ];
    let bare = vec![operator(1, "Pick one.")];

    assert_eq!(
        run(&room, &state(), &private, &policy),
        run(&room, &state(), &bare, &policy),
    );
    assert_eq!(spoke(run(&room, &state(), &private, &policy)).1.spent, 1);
}

#[test]
fn shifting_desk_sequences_past_a_private_row_can_change_the_step() {
    // The limit of the invariance guarantee, pinned rather than assumed.
    //
    // `step` ignores a private row: it resolves no trace, joins no standing and
    // costs no budget. What it cannot ignore is the *sequence* that row
    // consumed. A host allocating sequences live pushes every later desk row up
    // by one per private row, and both `QuorumPolicy::window` and salience
    // decay read a raw sequence distance — so a support that was inside the
    // window can fall outside it, and the episode legitimately decides
    // something else.
    //
    // The two transcripts differ by exactly one row, allocated exactly as a
    // host would: the aside takes sequence 2, so the support that would have
    // been sequence 2 becomes sequence 3.
    let room = Room::new();
    let policy = EpisodePolicy {
        quorum: QuorumPolicy {
            threshold: 2,
            window: 1,
            ..sequential().quorum
        },
        blind_round: false,
        ..sequential()
    };
    let unshifted = vec![
        said(1, "planner", "!propose #stage"),
        said(2, "critic", "!support #stage ^1"),
    ];
    let shifted = vec![
        said(1, "planner", "!propose #stage"),
        aside(2, "planner", &["scout"], "!aside @scout Between us."),
        said(3, "critic", "!support #stage ^1"),
    ];

    // The proposal is inside a one-sequence window in the unshifted transcript
    // and has aged out of it in the shifted one, so the room is at quorum in
    // the first and still deliberating in the second. Same desk rows, same
    // order, different `step`.
    assert_ne!(
        run(&room, &state(), &unshifted, &policy),
        run(&room, &state(), &shifted, &policy),
    );

    // And the difference is the *shift*, not the row: the same aside parked at
    // a sequence that displaces nothing leaves the step exactly as it was.
    let mut parked = unshifted.clone();
    parked.push(aside(9, "planner", &["scout"], "!aside @scout Between us."));
    assert_eq!(
        run(&room, &state(), &unshifted, &policy),
        run(&room, &state(), &parked, &policy),
    );
}
