# Process tree

Decides which processes belong to one instance, and signals only those. Linux
only.

| File | Purpose |
| --- | --- |
| `mod.rs` | `Tree`: evidence, sampling, scanning and pidfd signals. |
| `test.rs` | A fabricated `/proc` for attribution and pid reuse; real processes for signals. |
