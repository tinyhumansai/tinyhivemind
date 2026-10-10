# deepswe package

Static roles, seat prompts and routing values from the corresponding
`examples/openhuman/src/bin/deepswe_hive.rs` room. JSON policies use the language
version 1 defaults except the example's routing settings. The named host templates resolve runtime IDs, working directories and MCP
executables/arguments. Their config preserves iteration and temperature knobs.
Runtime providers,
credentials and sandbox processes are supplied by the host.

| Path | Purpose |
| --- | --- |
| `hive.json` | Roster, roles, desk and runtime policies |
| `constitution.json` | Host-owned edit rules and budgets |
| `memory.json` | Empty memory declarations for these example rooms |
| `seats/` | Per-seat prompts |
| `roles/` | Independent role charters |
