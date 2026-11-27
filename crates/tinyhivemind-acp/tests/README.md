# Tests

| Path | Purpose |
| --- | --- |
| `hive.rs` | `live_*`: a Claude Code seat, a Codex seat, and a mixed hive of OpenHuman, Claude Code and Codex, each in a real hive with a real tool server; then the same three editing files only through the host's `write_file` tool, each in its own workspace: a retried write runs once, writes that climb out of the workspace or are absolute are refused, a supervised seat's writes go to the host's approver, and a read-only seat and a denied seat write nothing; and lead-only episodes, one per harness as the lead (in `hive/delegation.rs`). Opted into by `THM_NODE`, `THM_ACP_CLAUDE`, `THM_ACP_CODEX`, `THM_CODEX` and `THM_OPENHUMAN_CORE`. |
| [`hive/`](hive/README.md) | The scripted model the hive tests play, and the lead-only episodes |
