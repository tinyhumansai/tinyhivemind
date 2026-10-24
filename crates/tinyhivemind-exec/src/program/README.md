# `program`

| File | Holds |
| --- | --- |
| `mod.rs` | `ProgramTool`, a `tinytools::Tool` over one `ExecTool` |
| [`test/`](test/README.md) | Its metadata, permission classes, and reported failures |

The model sees the declaration's `id`, `description` and `args`. The
permission gate sees `side_effects`: `read` is `PermissionLevel::ReadOnly`
with a classified read-only policy, `write` is `PermissionLevel::Write` with
the default policy, and `network` is `PermissionLevel::Write` with a
classified policy declaring network I/O. Every call gets a fresh
`THM_CALL_ID` of `<id>-<n>`. Any failure is returned as
`ToolResult::error` carrying the error's message, so the model reads it; a
truncated result ends with a note saying so.
