# validation/capability

The capability table judged seat by seat, default-deny: every field a seat
runs with is walked against its default through `harness/fields.rs`, and a
field with no decision is rejected.

| File | Purpose |
| --- | --- |
| `mod.rs` | The walker, the by-value rules, the shared-OpenClaw provider rule and the reserved `turn`/`call` arguments |
| `test.rs` | A field with no capability decision is rejected on every kind |
