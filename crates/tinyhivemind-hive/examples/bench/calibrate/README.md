# `calibrate/`

Tests for [`../calibrate.rs`](../calibrate.rs), which fits the cost model's four
constants to a live endpoint under `--calibrate`.

The request loop itself needs an endpoint and is deliberately not tested here,
the way the other live backends are treated. What *is* tested is the arithmetic
that turns two probes into four constants, because that is where a calibration
goes quietly wrong rather than loudly.

| file | what it holds |
| --- | --- |
| `test.rs` | that the fit rounds to nearest rather than truncating; that a zero denominator reads as zero rather than dividing; and that each probe pair differs in exactly one thing — the row count, or the completion length asked for — since a pair differing in two would attribute both to whichever one is being fitted |
