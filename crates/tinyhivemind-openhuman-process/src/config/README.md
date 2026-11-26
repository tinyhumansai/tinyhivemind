# Config

The `config.toml` of a seat's instance, built as one JSON tree from the spike's
`config.toml.in` and written as TOML. `Written::settings` names every leaf
written plus `api_key`, `api_url` and `inference_url`, which must stay unset;
`Written::observe` reads the same paths from a `config_get` snapshot, cutting
lists of tables down to the keys written. The binary falls back to defaults on
TOML it cannot read, so that comparison is the only proof the config took.

Not written: the top-level `embeddings_provider`, which 0.64.15 rewrites to
its hosted default on load whatever it says; memory is off, so nothing embeds.

| File | Purpose |
| --- | --- |
| `mod.rs` | `Input`, `Written`, `AGENT_ID`, the fixed settings and why |
| `toml.rs` | JSON as TOML, for the shapes the config has |
| `test.rs` | What the config says, that it parses back, and the comparison |
