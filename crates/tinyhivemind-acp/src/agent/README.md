# Agent

What differs between ACP agents, as data, with `{name}` templates filled per seat in one pass (a value is never expanded again), TOML-escaped in a `.toml` file. `write_codex_catalog` fails with a typed `CatalogError`.

| File | Purpose |
| --- | --- |
| `mod.rs` | `Agent`, `Agent::claude_code`, `Agent::codex`, `write_codex_catalog`, template values |
| `test.rs` | Templates, TOML escaping, the two descriptors and each catalog failure |
