//! Unit tests for the token and wall-clock cost model.
//!
//! Three properties carry the whole module, and each is a thing a reader of
//! the table is entitled to assume: a round's wall clock does not grow with
//! its width, a sample's cost folds the same however the threads split it, and
//! a flag that cannot be parsed stops the run rather than quietly measuring
//! something else.

use super::{Cost, CostModel, RoundShape};

/// The model every test below prices against, with round numbers so an
/// expected value can be checked by hand rather than by rerunning the code
/// under test.
const MODEL: CostModel = CostModel {
    tokens_per_row: 10,
    tokens_per_turn: 100,
    prompt_base: 500,
    ttft_ms: 500,
    decode_tok_s: 100,
};

fn args(values: &[&str]) -> impl Iterator<Item = String> + use<> {
    values
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>()
        .into_iter()
}

#[test]
fn charges_a_turn_its_base_prompt_plus_a_token_per_row() {
    let cost = MODEL.price(&[RoundShape { rows: 3, turns: 1 }]);
    assert_eq!(cost.prompt, 500 + 3 * 10);
    assert_eq!(cost.completion, 100);
    assert_eq!(cost.tokens(), 630);
}

#[test]
fn a_wider_round_costs_more_tokens_but_no_more_wall_clock() {
    let narrow = MODEL.price(&[RoundShape { rows: 4, turns: 1 }]);
    let wide = MODEL.price(&[RoundShape { rows: 4, turns: 4 }]);

    // The reason depth and width are counted separately: turns in one round
    // are authorized against the same transcript and cannot read each other,
    // so a host with a seat for each waits once for all of them.
    assert_eq!(wide.wall_ms, narrow.wall_ms);
    assert_eq!(wide.tokens(), narrow.tokens() * 4);
    assert!((wide.concurrency() - 4.0).abs() < f64::EPSILON);
    assert!((narrow.concurrency() - 1.0).abs() < f64::EPSILON);
}

#[test]
fn wall_clock_is_a_sum_over_rounds_not_over_turns() {
    let deep = MODEL.price(&[
        RoundShape { rows: 0, turns: 1 },
        RoundShape { rows: 1, turns: 1 },
        RoundShape { rows: 2, turns: 1 },
    ]);
    let flat = MODEL.price(&[RoundShape { rows: 0, turns: 3 }]);

    assert_eq!(deep.turns, flat.turns);
    // 500 ms to first token plus 100 tokens at 100 tok/s, three times over.
    assert_eq!(deep.wall_ms, 3 * 1_500);
    assert_eq!(flat.wall_ms, 1_500);
}

#[test]
fn merging_two_samples_equals_pricing_one() {
    // The property that lets the per-room loops spread across `--jobs`
    // threads without moving a printed number, exactly as
    // `metrics::Aggregate::merge` is held to.
    let left = [RoundShape { rows: 1, turns: 2 }, RoundShape { rows: 3, turns: 1 }];
    let right = [RoundShape { rows: 5, turns: 4 }];

    let mut folded = MODEL.price(&left);
    folded.merge(MODEL.price(&right));

    let together = MODEL.price(&[left[0], left[1], right[0]]);
    assert_eq!(folded, together);
}

#[test]
fn an_empty_episode_reports_no_rate_rather_than_dividing_by_zero() {
    let empty = Cost::ZERO;
    assert!((empty.concurrency() - 0.0).abs() < f64::EPSILON);
    assert!((empty.tokens_per_second() - 0.0).abs() < f64::EPSILON);
    assert!((empty.episodes_per_hour(10) - 0.0).abs() < f64::EPSILON);
    assert!((empty.mean_ms(0) - 0.0).abs() < f64::EPSILON);
    assert!((empty.mean_tokens(0) - 0.0).abs() < f64::EPSILON);
}

#[test]
fn tokens_per_second_divides_the_whole_sample_by_its_wall_clock() {
    // One round: 630 tokens in 1.5 s.
    let cost = MODEL.price(&[RoundShape { rows: 3, turns: 1 }]);
    assert!((cost.tokens_per_second() - 630.0 / 1.5).abs() < 1e-9);
    assert!((cost.episodes_per_hour(1) - 3_600.0 / 1.5).abs() < 1e-9);
}

#[test]
fn sets_each_constant_from_its_own_flag() {
    let mut model = CostModel::DEFAULT;
    for (flag, value) in [
        ("--tokens-per-row", 7),
        ("--tokens-per-turn", 8),
        ("--prompt-base", 9),
        ("--ttft", 10),
        ("--decode-rate", 11),
    ] {
        let raw = value.to_string();
        assert_eq!(model.set(flag, &mut args(&[&raw])), Ok(true));
    }
    assert_eq!(
        model,
        CostModel {
            tokens_per_row: 7,
            tokens_per_turn: 8,
            prompt_base: 9,
            ttft_ms: 10,
            decode_tok_s: 11,
        }
    );
}

#[test]
fn leaves_a_flag_it_does_not_own_for_somebody_else() {
    let mut model = CostModel::DEFAULT;
    assert_eq!(model.set("--seed", &mut args(&["3"])), Ok(false));
    assert_eq!(model, CostModel::DEFAULT);
}

#[test]
fn refuses_a_value_it_cannot_parse_rather_than_defaulting_it() {
    // A nonnumeric value read as zero would price the run at no tokens, or at
    // no latency, and would additionally swallow the next flag as its
    // argument -- a benchmark measuring something other than what was asked
    // for, printed under the heading that was.
    let mut model = CostModel::DEFAULT;
    assert!(model.set("--ttft", &mut args(&["--swarm"])).is_err());
    assert!(model.set("--tokens-per-row", &mut args(&[])).is_err());
    assert_eq!(model, CostModel::DEFAULT);
}

#[test]
fn refuses_a_decode_rate_of_zero_because_it_divides() {
    let mut model = CostModel::DEFAULT;
    assert!(model.set("--decode-rate", &mut args(&["0"])).is_err());
    assert_eq!(model, CostModel::DEFAULT);
}

#[test]
fn the_header_names_every_flag_that_would_reproduce_it() {
    let header = MODEL.header();
    for flag in [
        "--tokens-per-row",
        "--tokens-per-turn",
        "--prompt-base",
        "--ttft",
        "--decode-rate",
    ] {
        assert!(header.contains(flag), "header omits {flag}: {header}");
    }
}
