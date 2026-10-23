# deepswe_hive package

The `deepswe` hive: `lead`, `implementer`, `tester` and `reviewer`, each
allowed only `mcp_list_tools` and `mcp_call_tool`, 16 iterations, temperature
zero. Every seat connects to the shared `deepswe` workspace server and to its
own `tinyhive-<seat>` hive tool server; the binary fills in their executable
path and arguments per run, along with the deployment id, workspace and model
name. Episode, routing and coordinator widths are four, within
`runtime.concurrency`. See the [package index](../README.md).

| File | Purpose |
| --- | --- |
| `hive.json` | Seats, desk, policies, runtime, model and MCP servers, in canonical form |
| `constitution.json` | Default caps: 60 turns, width 8 |
| `memory.json` | Memory root; no identities or bindings |
| `roles/` | One charter per role |
| `seats/` | One prompt per seat, naming that seat's hive tool server |
| `context/` | The shared protocol note no seat composes; see its README |
