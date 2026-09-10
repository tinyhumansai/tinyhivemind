//! Unit tests for the calibration fit.
//!
//! The probes themselves need an endpoint and are therefore not tested here —
//! the arithmetic that turns two probes into four constants is, because that
//! is where a calibration goes quietly wrong rather than loudly.

use super::{PROBE_COMPLETION, PROBE_ROWS, div_round, probe_prompt};

#[test]
fn rounds_to_nearest_rather_than_truncating() {
    // A per-row cost of 41.6 truncated to 41 is a 1.4% error on every row of
    // every prompt in the table, in the same direction every time.
    assert_eq!(div_round(7, 2), 4);
    assert_eq!(div_round(5, 2), 3);
    assert_eq!(div_round(4, 2), 2);
    assert_eq!(div_round(1, 3), 0);
}

#[test]
fn a_zero_denominator_reads_as_zero_rather_than_dividing() {
    assert_eq!(div_round(9, 0), 0);
}

#[test]
fn the_two_prompt_probes_differ_only_in_how_many_rows_they_carry() {
    // What makes the slope between them the per-row cost and nothing else: if
    // the two probes differed in their instructions as well as their rows,
    // the fit would attribute that difference to the rows.
    let short = probe_prompt(PROBE_ROWS.0, PROBE_COMPLETION.0);
    let long = probe_prompt(PROBE_ROWS.1, PROBE_COMPLETION.0);
    assert!(long.len() > short.len());

    let rows_of = |prompt: &str| prompt.matches("!propose").count();
    assert_eq!(rows_of(&short), PROBE_ROWS.0);
    assert_eq!(rows_of(&long), PROBE_ROWS.1);

    // Same preamble, same instruction, different only in the middle.
    let head = "You are one seat on a desk";
    assert!(short.starts_with(head) && long.starts_with(head));
    let tail = "Do not use lists.";
    assert!(short.ends_with(tail) && long.ends_with(tail));
}

#[test]
fn the_two_latency_probes_differ_only_in_the_length_they_ask_for() {
    let brief = probe_prompt(PROBE_ROWS.0, PROBE_COMPLETION.0);
    let verbose = probe_prompt(PROBE_ROWS.0, PROBE_COMPLETION.1);
    assert_eq!(
        brief.matches("!propose").count(),
        verbose.matches("!propose").count()
    );
    assert!(brief.contains(&PROBE_COMPLETION.0.to_string()));
    assert!(verbose.contains(&PROBE_COMPLETION.1.to_string()));
}

#[test]
fn every_row_of_a_probe_is_the_same_length() {
    // The quantity being fitted is tokens *per row*. Rows of varying length
    // would fit the average length of this harness's filler instead of the
    // endpoint's tokenisation of a row.
    let prompt = probe_prompt(12, PROBE_COMPLETION.0);
    let lengths: Vec<usize> = prompt
        .lines()
        .filter(|line| line.contains("!propose"))
        .map(|line| line.chars().filter(|c| !c.is_ascii_digit()).count())
        .collect();
    assert_eq!(lengths.len(), 12);
    assert!(
        lengths.windows(2).all(|pair| pair[0] == pair[1]),
        "probe rows are not uniform: {lengths:?}"
    );
}

#[test]
fn the_probe_lengths_are_far_enough_apart_to_fit_a_slope() {
    // Two probes a few rows or a few tokens apart fit a slope dominated by
    // the tokeniser's own noise, which is how a calibration produces a
    // confident wrong number.
    assert!(PROBE_ROWS.1 >= PROBE_ROWS.0 * 10);
    assert!(PROBE_COMPLETION.1 >= PROBE_COMPLETION.0 * 10);
}
