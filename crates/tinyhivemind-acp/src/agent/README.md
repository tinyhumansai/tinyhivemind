# Agent

What differs between ACP agents, as data, with `{name}` templates filled per seat in one pass (a value is never expanded again), TOML-escaped in a `.toml` file. `write_codex_catalog` gives each seat's model its own catalog entry (a routed model's, or the first's without reasoning and verbosity) and fails with a typed `CatalogError`. Codex offers MCP tools as `namespace` tools, which its Responses endpoint must accept.

| File | Purpose |
| --- | --- |
| `mod.rs` | `Agent`, `Agent::claude_code`, `Agent::codex`, `write_codex_catalog`, template values (`Agent::openclaw` is in [`../openclaw/`](../openclaw/README.md)) |
| `test.rs` | Templates, TOML escaping, the two descriptors, each model's catalog entry and each catalog failure |
