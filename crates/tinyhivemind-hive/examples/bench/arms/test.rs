//! Tests for the control arms' own pricing.
//!
//! What the arms *decide* is covered by the comparison as a whole. What is
//! covered here is what they are charged, because a control that is priced
//! too cheaply makes every arm measured against it look worse than it is.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{blind_shape, routed_shape};

#[test]
fn a_ladder_that_had_to_ask_who_answers_pays_for_the_asking() {
    // The `Select` rung puts the candidate list to a router, which is a model
    // call like any other. Charging only the responder's turn reported the
    // routed ladder at half its real latency and roughly half its tokens.
    let routed = routed_shape(true);
    let decided = routed_shape(false);

    assert_eq!(routed.len(), 2, "the router's call is a round of its own");
    assert_eq!(
        decided.len(),
        1,
        "a ladder that never asked pays for one turn"
    );

    // Sequential, not one wide round: nobody can answer until the router has
    // said who answers, so a host waits for the two in series.
    let turns: usize = routed.iter().map(|round| round.rows.len()).sum();
    assert_eq!(turns, 2);
    assert!(routed.iter().all(|round| round.rows.len() == 1));
}

#[test]
fn a_ladder_that_never_asked_is_priced_exactly_as_before() {
    // The `Decided` rung answers from the roster alone and spends nothing, so
    // it must stay bit-identical to the one-turn shape it always had.
    assert_eq!(routed_shape(false), blind_shape(1));
}

#[test]
fn a_blind_arm_of_no_turns_has_no_shape_to_price() {
    // `vote` with a zero budget takes no turns; a round of nothing would be
    // charged a round of wall clock it never waited.
    assert!(blind_shape(0).is_empty());
}

#[test]
fn every_turn_of_a_blind_arm_reads_only_the_brief() {
    // A control arm's members answer from their own private evaluation alone,
    // having seen nothing but the operator's row -- which is what makes the
    // poll cheap in wall clock and dear in tokens at once.
    let shape = blind_shape(15);
    assert_eq!(shape.len(), 1, "a poll is one round, however wide");
    assert_eq!(shape[0].rows.len(), 15);
    assert!(shape[0].rows.iter().all(|rows| *rows == 1));
}
