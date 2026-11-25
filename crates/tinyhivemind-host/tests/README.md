# Integration tests

| Path | Purpose |
| --- | --- |
| `conformance.rs` | The conformance suite over the fake harness, and one deliberately broken fake per rule. |
| [`fake/`](fake/README.md) | The fake harness. |

Run with `cargo test -p tinyhivemind-host --all-features`; the suite needs the
`conformance` feature and Linux.
