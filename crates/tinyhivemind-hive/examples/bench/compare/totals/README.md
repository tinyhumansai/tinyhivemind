# `compare/totals/`

Tests for [`../totals.rs`](../totals.rs), the per-arm aggregates the comparison
folds into.

| file | what it holds |
| --- | --- |
| `test.rs` | that every arm in a run — `ladder_directed` included, which sits outside `arms_mut`'s array — is priced at the cost model the run selected, so no row can report against different constants from the header printed above it |
