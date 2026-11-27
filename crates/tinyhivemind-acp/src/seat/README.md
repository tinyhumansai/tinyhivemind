# Seat

One seat: its agent process in a private directory, its one session (new, or loaded after a restart), the turn it runs, cancel and stop. An agent is used only if its `initialize` says protocol version 1 and HTTP MCP; a session is loaded only if it says `loadSession`. A cancel answers the turn's pending permission requests `cancelled`, and a turn's route is closed, and what it held answered, even if its future is dropped. ACP has no system prompt, so the first turn a process runs in a session carries the seat's prompt ahead of the rendered turn, until one such turn completes. A session loaded after a restart is shown it again, since one opened before seats were shown their prompt never was.

| File | Purpose |
| --- | --- |
| `mod.rs` | `Seat`, `Live`, launch and verification, sessions, cancel, stop, `Runner` |
| `turn.rs` | One `session/prompt`: progress, permission answers, reply and usage; a result without a `stopReason` is a broken protocol |
| `test.rs` | The seat's prompt ahead of a session's first turn |
