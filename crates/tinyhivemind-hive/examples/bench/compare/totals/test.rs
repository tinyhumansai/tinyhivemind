//! Unit tests for [`super::Totals`]: that pricing the whole table reaches
//! every arm, `ladder_directed` included.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::Totals;
use crate::cost::CostModel;

/// A cost model with every constant moved off [`CostModel::DEFAULT`], so a
/// row still carrying the default after `priced_at` is easy to catch.
const MOVED: CostModel = CostModel {
    tokens_per_row: 7,
    tokens_per_turn: 8,
    prompt_base: 9,
    ttft_ms: 10,
    decode_tok_s: 11,
};

/// `ladder_directed` sits outside [`Totals::arms_mut`]'s array because its
/// per-room work depends on a directory earned over prior episodes, and
/// `priced_at` used to price only that array -- leaving `ladder_directed` at
/// `Aggregate::default()`'s model, `CostModel::DEFAULT`, regardless of what a
/// `--tokens-per-*`, `--ttft` or `--decode-rate` flag asked for. Every row
/// the table prints, this one included, must be priced at the model
/// `priced_at` was actually given.
#[test]
fn priced_at_reaches_every_arm_ladder_directed_included() {
    let totals = Totals::priced_at(MOVED);
    for arm in totals.arms() {
        assert_eq!(
            arm.model, MOVED,
            "an arm in Totals::arms() was not priced at the model priced_at was given",
        );
    }
    assert_eq!(
        totals.ladder_directed.model, MOVED,
        "ladder_directed must be priced at the model priced_at was given, not left at the default",
    );
}
