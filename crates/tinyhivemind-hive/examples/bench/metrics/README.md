# Metrics

Aggregation and the tables the benchmark prints. Every rate is reported over
the whole sample, including episodes that never decided — an arm cannot buy
accuracy by declining to answer. See [`../README.md#statistics`](../README.md#statistics)
and [`../STATISTICS.md`](../STATISTICS.md) for what each printed column means
and what its confidence interval does and does not license.

| file | what it does |
| --- | --- |
| [`mod.rs`](mod.rs) | `Aggregate` and the fold that builds one from a run of `EpisodeReport`s; re-exports everything a caller outside this module needs. |
| [`stats.rs`](stats.rs) | Small numeric statistics: widening a `u64` too large for an exact `f64` cast, a percentile out of a sorted sample, and rank-tie arithmetic for `spearman_milli`. |
| [`tables.rs`](tables.rs) | The printed and `--json` tables themselves: the six-column headline row, the library-cost row beside it, the detail and cost tables, and the paired-difference lines. Extracted from `mod.rs` when the new columns pushed that file past the 700-line cap. |
| [`format.rs`](format.rs) | Renders an `Aggregate` as a fixed-width table cell, a dash for a column an arm structurally cannot have, or a JSON number/`null` for `--json`. Computes nothing itself. |
