# Adapter errors

`Error` names OpenHuman failures, so it stays here. A `tinyhivemind_host::Error`
becomes the variant adapter callers always matched, and an adapter error that
crossed the host's belt — a policy refusal, a factory failure — comes back as
itself. `ConfigError` is the host's, re-exported from the crate root.

| File | Responsibility |
| --- | --- |
| `mod.rs` | `Error`, `Result` and the conversions |
| `test.rs` | Variants and text survive both directions |
