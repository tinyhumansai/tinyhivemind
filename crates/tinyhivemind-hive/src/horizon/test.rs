//! Unit tests for the two distance bases and the window they share.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

/// Four folded rows, ten sequences apart, in a transcript that is not dense.
fn rows() -> Vec<Sequence> {
    vec![Sequence(10), Sequence(20), Sequence(30), Sequence(40)]
}

#[test]
fn a_bare_sequence_measures_raw_distance() {
    let horizon = Horizon::from(Sequence(40));
    assert_eq!(horizon.sequence(), Sequence(40));
    assert_eq!(horizon.distance(Sequence(40)), 0);
    assert_eq!(horizon.distance(Sequence(10)), 30);
}

#[test]
fn live_rows_measure_folded_distance() {
    let rows = rows();
    let horizon = Horizon::over(Sequence(40), &rows);
    assert_eq!(horizon.distance(Sequence(40)), 0);
    assert_eq!(horizon.distance(Sequence(30)), 1);
    assert_eq!(horizon.distance(Sequence(10)), 3);
}

#[test]
fn a_dense_journal_measures_the_same_either_way() {
    // The case the benchmark is in: every row above the watermark is folded,
    // so there is nothing for the two rulers to disagree about.
    let dense: Vec<Sequence> = (1..=40).map(Sequence).collect();
    let ranked = Horizon::over(Sequence(40), &dense);
    let raw = Horizon::from(Sequence(40));
    for sequence in (1..=40).map(Sequence) {
        assert_eq!(ranked.distance(sequence), raw.distance(sequence));
        assert_eq!(ranked.within(sequence, 30), raw.within(sequence, 30));
    }
}

#[test]
fn a_window_admits_a_row_the_raw_ruler_would_have_dropped() {
    // Thirty sequences of transcript carrying four folded rows. A window of
    // three admits the whole episode when it is measured in rows and only the
    // last row when it is measured in sequences, which is the difference a
    // busy desk sees on an unchanged policy.
    let rows = rows();
    assert!(!Horizon::from(Sequence(40)).within(Sequence(10), 3));
    assert!(Horizon::over(Sequence(40), &rows).within(Sequence(10), 3));
}

#[test]
fn nothing_after_the_horizon_is_ever_in_window() {
    let rows = rows();
    for horizon in [
        Horizon::from(Sequence(20)),
        Horizon::over(Sequence(20), &rows),
    ] {
        assert!(!horizon.within(Sequence(30), 1_000));
        // ...and it is zero distance rather than a negative one.
        assert_eq!(horizon.distance(Sequence(30)), 0);
    }
}

#[test]
fn a_sequence_the_fold_never_read_lands_where_it_would_have() {
    // Sequence 25 is not a folded row. Inserting it would put it where row 30
    // sits, so it measures as row 30 does: one row back from the horizon. The
    // point is that it is monotone and has no special case, not that an
    // unfolded sequence has a meaningful distance of its own.
    let rows = rows();
    let horizon = Horizon::over(Sequence(40), &rows);
    assert_eq!(horizon.distance(Sequence(30)), 1);
    assert_eq!(horizon.distance(Sequence(25)), 1);
    assert_eq!(horizon.distance(Sequence(21)), 1);
    assert_eq!(horizon.distance(Sequence(20)), 2);
}

#[test]
fn an_empty_row_set_measures_everything_as_here() {
    // No folded rows means no distance to measure: the window admits whatever
    // is at or before the horizon, which is what an episode that has folded
    // nothing should say.
    let horizon = Horizon::over(Sequence(40), &[]);
    assert_eq!(horizon.distance(Sequence(1)), 0);
    assert!(horizon.within(Sequence(1), 0));
}

#[test]
fn the_basis_pins_its_wire_form() {
    assert_eq!(
        serde_json::to_value(Basis::Sequence).expect("serializes"),
        serde_json::json!("sequence"),
    );
    assert_eq!(
        serde_json::to_value(Basis::Live).expect("serializes"),
        serde_json::json!("live"),
    );
    assert_eq!(Basis::default(), Basis::Sequence);
}
