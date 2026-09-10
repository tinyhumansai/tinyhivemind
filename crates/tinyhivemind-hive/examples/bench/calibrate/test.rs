//! Unit tests for the calibration fit.
//!
//! The probes themselves need an endpoint and are therefore not tested here —
//! the arithmetic that turns two probes into four constants is, because that
//! is where a calibration goes quietly wrong rather than loudly.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{PROBE_COMPLETION, PROBE_ROWS, div_round, probe_prompt};

/// Text carried by a probe's filler rows and by nothing in the fixed prompt
/// skeleton, so counting it counts rows and only rows.
const ROW_MARK: &str = "blast radius";

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
    // the two probes differed in their instructions as well as their rows, the
    // fit would attribute that difference to the rows.
    let short = probe_prompt(PROBE_ROWS.0, PROBE_COMPLETION.0);
    let long = probe_prompt(PROBE_ROWS.1, PROBE_COMPLETION.0);
    assert!(long.len() > short.len());

    // Counted on text unique to the filler rows. `!propose` will not do: the
    // protocol grammar block names every marker, so it appears in the fixed
    // skeleton too and a count of it mixes rows with the intercept.
    let rows_of = |prompt: &str| prompt.matches(ROW_MARK).count();
    assert_eq!(rows_of(&short), PROBE_ROWS.0);
    assert_eq!(rows_of(&long), PROBE_ROWS.1);

    // Same skeleton on both sides of the transcript, so everything outside the
    // rows cancels in the subtraction.
    let head = |prompt: &str| {
        prompt
            .split("Shared attributed transcript:")
            .next()
            .map(str::to_owned)
    };
    assert_eq!(head(&short), head(&long));
    assert!(short.ends_with("Your one line:") && long.ends_with("Your one line:"));
}

#[test]
fn a_probe_is_built_from_the_prompt_a_live_seat_actually_gets() {
    // `prompt_base` is defined as the whole non-transcript cost of a turn, so
    // fitting it from a preamble of this module's own would measure this
    // module rather than the benchmark and underprice every turn. The probe
    // goes through `AgentPrompt::prompt_with`, so the identity line, the
    // private-facts block and the protocol grammar are all inside the
    // intercept where they belong.
    let prompt = probe_prompt(PROBE_ROWS.0, PROBE_COMPLETION.0);
    assert!(
        prompt.contains("You are @planner"),
        "no identity line: {prompt}"
    );
    assert!(
        prompt.contains("reverted once before"),
        "no private-facts block: {prompt}"
    );
    assert!(prompt.contains("!commit"), "no protocol grammar: {prompt}");
    assert!(prompt.contains("Shared attributed transcript:"));
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
    // would fit the mean length of this harness's filler instead of the
    // endpoint's tokenisation of a row.
    let prompt = probe_prompt(12, PROBE_COMPLETION.0);
    let lengths: Vec<usize> = prompt
        .lines()
        .filter(|line| line.contains(ROW_MARK))
        // Sequence numbers and any other digits vary by row and are not part
        // of what is being held constant.
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
