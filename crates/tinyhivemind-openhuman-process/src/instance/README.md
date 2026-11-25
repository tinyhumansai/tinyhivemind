# Instances

`Launcher::launch` makes the seat's private directories (`ws/`, `home/`,
`action/`, the keyring key), starts a discovery instance whose allowlist
matches nothing, waits out the MCP warm-up and reads each hive tool's hashed
name, stops it, writes the exact allowlist and starts again. `verify` then
compares `/health.pid` with the child, `core.version` with the pin, `config_get`
with what was written, and the model-visible tools with the hive tools: the
agent's effective allowlist of the tools the instance has, plus
`juice_retrieve` unless `context.compaction_enabled` is off. Any difference
stops the instance and fails the start.

| File | Purpose |
| --- | --- |
| `mod.rs` | `Launcher`, `Live`, `Layout`, the environment |
| `verify.rs` | `discover`, `verify` |
| `test.rs` | Against the fake surface, and the exact environment |
