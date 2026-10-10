# Adapter modules

| Path | Responsibility |
| --- | --- |
| `host/` | Supplied handle registration, continuing sessions, host hooks and management |
| `tools/` | Stable native specifications, argument validation and bound execution |
| `memory/` | Hive-shared OpenHuman memory: the hive root, per-seat memory agent ids, seat binding, and core `Recall`/`Remember` |
| `language/` | Portable definition conversions for coordinator settings, seat requests and memory bindings |
| `journal/` | Optional in-memory host log used by standalone research examples |
| `offline/` | Feature gated loopback backend and host runtime configuration fixtures |
| `error.rs` | Typed adapter errors |
| `lib.rs` | Public exports and registration example |

Agent construction, MCP connections, skills, the memory engine and original
prompts belong to the host; `memory/` only decides which root and memory agent
id each seat is bound to. Durable scheduling and episode state belong to `tinyhivemind-hives`.
