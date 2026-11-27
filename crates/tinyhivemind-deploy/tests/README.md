# Tests

| Path | Purpose |
| --- | --- |
| `hive.rs` | `live_*`: a Claude Code seat, a Codex seat and an OpenClaw seat, a mixed hive of OpenHuman, Claude Code and Codex, and one of all four, each started on its harness by `ProcessSeats` in a real hive with a real tool server. The shared fixtures: members by harness kind, the installs from the environment, and starting, settling and stopping a hive. Opted into by `THM_NODE`, `THM_ACP_CLAUDE`, `THM_ACP_CODEX`, `THM_CODEX`, `THM_OPENHUMAN_CORE` and `THM_OPENCLAW` (OpenClaw's `openclaw.mjs`). |
| [`hive/`](hive/README.md) | The scripted model the hive tests play, the edit tests and the lead-only episodes |
