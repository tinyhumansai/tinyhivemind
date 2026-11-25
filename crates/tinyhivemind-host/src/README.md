# Source layout

| Path | Purpose |
| --- | --- |
| `lib.rs` | Crate overview and the public surface |
| [`tools/`](tools/README.md) | `Kind`, `ToolCall` and `Belt`: the hive tools and their execution |
| [`policy/`](policy/README.md) | `SendAuthorizer`, `ManagementAuthorizer`, `AgentCreator` and the requests they see |
| [`turn/`](turn/README.md) | `TurnScope`, `TurnOptions` and the prompt |
| [`language/`](language/README.md) | `PackageSource`, package lowering and portable conversions |
| [`config/`](config/README.md) | The deployment plan and its validation |
| [`activation/`](activation/README.md) | The registration barrier |
| [`journal/`](journal/README.md) | `MemoryLog`, an optional in-memory session log |
| [`error/`](error/README.md) | `Error`, `ArgumentError` and `Result` |
| `process/` | Supervised child processes |
| `harness/` | The harness port |
