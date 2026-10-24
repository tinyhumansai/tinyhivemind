# Source layout

| Path | Purpose |
| --- | --- |
| `lib.rs` | Crate overview and the public surface: `run`, `Output`, `ExecContext`, `Secrets`, `EnvSecrets`, `ProgramTool`, `HookRunner`, `Verdict`, `Error`, `Result`. |
| [`context/`](context/README.md) | `ExecContext`, the `Secrets` port and `EnvSecrets`. |
| [`error/`](error/README.md) | `Error`, every way a call fails, and the `Result` alias. |
| [`run/`](run/README.md) | `run` and `Output`: one call under the calling convention. |
| [`program/`](program/README.md) | `ProgramTool`, a declaration as a native `tinytools::Tool`. |
| [`hook/`](hook/README.md) | `HookRunner` and `Verdict`: a seat's hooks for one event. |
