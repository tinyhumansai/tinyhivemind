# Process tree

Decides which processes belong to one instance, and signals only those. Linux
only.

| File | Purpose |
| --- | --- |
| `mod.rs` | `Tree`: evidence, sampling and pidfd signals. |
| `scan.rs` | Finding every member, and the argument for when there is none. |
| `stat.rs` | One process as `/proc` describes it; only `ENOENT` and `ESRCH` mean gone. |
| [`test/`](test/README.md) | A fabricated `/proc` for attribution and pid reuse; real processes for signals. |
