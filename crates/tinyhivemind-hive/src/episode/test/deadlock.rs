//! Deadlock, and cross-inhibition through the whole machine: two carried
//! topics are terminal only once nobody remains who has backed neither, and a
//! grounded objection can silence an advocate and carry the room straight
//! through.

use super::super::*;
use super::support::{spoke, Room, deadlocked, member, run, said, sequential, state};
use crate::trace::TopicId;

#[test]
fn a_deadlock_a_dissenter_can_still_break_authorizes_one_more_turn() {
    let mut room = Room::new();
    // A fourth member who has backed neither side is free to break the tie.
    room.members.push(member("archivist"));
    room.desks[0].members.push("archivist".into());

    let step = run(&room, &state(), &deadlocked(), &sequential());
    assert!(
        matches!(step, HiveStep::Speak { .. }),
        "while a member who has backed neither side exists, the room is not \
         deadlocked; it gets another turn — got {step:?}",
    );

    // The same room without that member is terminal, so the dissenter is what
    // makes the difference rather than anything else in the transcript.
    let committed = Room::new();
    assert_eq!(
        run(&committed, &state(), &deadlocked(), &sequential()),
        HiveStep::Deadlocked {
            topics: vec![TopicId("stage".into()), TopicId("ship".into())],
        },
    );
}

#[test]
fn addressed_precedence_does_not_mask_an_available_dissenter() {
    // archivist backs neither tied topic, so the room must stay open — but
    // archivist's own message is also targeted by a later trace, which would
    // classify archivist's bid as `Addressed` rather than `Dissent`. The
    // terminal check must see the dissent structurally, from the standings,
    // rather than through that bid-reason precedence.
    let mut room = Room::new();
    room.members.push(member("archivist"));
    room.desks[0].members.push("archivist".into());

    let mut transcript = deadlocked();
    transcript.push(said(5, "archivist", "!question What about latency?"));
    transcript.push(said(6, "critic", "!object >5 Out of scope."));

    let step = run(&room, &state(), &transcript, &sequential());
    assert!(
        matches!(step, HiveStep::Speak { .. }),
        "archivist is free to break the tie even though addressed, got {step:?}",
    );
}

#[test]
fn a_deadlock_nobody_can_break_is_terminal() {
    // In `deadlocked()` every member has taken a side: planner backs both,
    // critic backs `stage`, scout backs `ship`. Nobody is left to break it.
    let room = Room::new();
    assert_eq!(
        run(&room, &state(), &deadlocked(), &sequential()),
        HiveStep::Deadlocked {
            topics: vec![TopicId("stage".into()), TopicId("ship".into())],
        },
    );
}

#[test]
fn a_grounded_objection_carries_the_room_through_a_deadlock() {
    let mut room = Room::new();
    room.desks[0].members = vec!["planner".into(), "scout".into(), "critic".into()];
    let mut transcript = deadlocked();
    transcript.push(said(5, "critic", "!object >4 ^3 That precedent differs."));

    let (turn, next) = spoke(run(&room, &state(), &transcript, &sequential()));
    assert_eq!(turn.phase, Phase::Commit);

    transcript.push(said(6, &turn.agent_id, "!commit #stage Locking this in."));
    let HiveStep::Converged { topic, .. } = run(&room, &next, &transcript, &sequential()) else {
        panic!("expected convergence")
    };
    assert_eq!(topic, TopicId("stage".into()));
}
