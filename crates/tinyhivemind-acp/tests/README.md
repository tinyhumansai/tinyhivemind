# Tests

| Path | Purpose |
| --- | --- |
| `hive.rs` | `live_*`: a Claude Code seat, a Codex seat, and a mixed hive of OpenHuman, Claude Code and Codex, each in a real hive with a real tool server; then the same three editing files through the host's `write_file` tool, each in its own workspace, beside a read-only seat whose write the host refuses. Opted into by `THM_NODE`, `THM_ACP_CLAUDE`, `THM_ACP_CODEX`, `THM_CODEX` and `THM_OPENHUMAN_CORE`. |
| [`hive/`](hive/README.md) | The scripted model the hive tests play |
