# Instances

`Launcher::launch` makes the seat's private directories (`ws/`, `home/`,
`action/`, the keyring key), starts a discovery instance whose allowlist
matches nothing, waits out the MCP warm-up and reads each hive tool's hashed
name, stops it, writes the exact allowlist and starts again. `verify` then
compares `/health.pid` with the child, `core.version` with the pin, `config_get`
with what was written, and the model-visible tools with the hive tools. No
RPC reports those, so `visible` computes them as 0.64.15's `resolved_scope`
does; see the spec's OpenHuman rules for every source it counts. Any
difference stops the instance and fails the start, and a stop that then fails
too is reported with it (`Error::Abandoned`). The keyring key file must be a
0600 file of 64 hex digits; another is refused, not replaced.

| File | Purpose |
| --- | --- |
| `mod.rs` | `Launcher`, `Live`, `Layout`, the environment |
| `verify.rs` | `discover`, `verify` |
| `test.rs` | Against the fake surface, and the exact environment |
