# `cost/`

Tests for the token and wall-clock cost model in [`../cost.rs`](../cost.rs) —
the module five of the benchmark's six headline columns are computed from.
[`../COST.md`](../COST.md) documents the model itself: what it assumes, what it
refuses to claim, and how `--calibrate` measures it.

| file | what it holds |
| --- | --- |
| `test.rs` | that a wider round costs more tokens and no more wall clock; that merging two priced samples equals pricing one, which is what lets the per-room loops run across `--jobs` threads without moving a printed number; and that a constant which will not parse leaves the model untouched rather than defaulting to zero |
