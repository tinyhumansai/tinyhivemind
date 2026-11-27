# Seat

One seat: its agent process in a private directory, its one session (new, or loaded after a restart), the turn it runs, cancel and stop. An agent is used only if its `initialize` says protocol version 1 and HTTP MCP; a session is loaded only if it says `loadSession`. A cancel answers the turn's pending permission requests `cancelled`, and a turn's route is closed, and what it held answered, even if its future is dropped.

| File | Purpose |
| --- | --- |
| `mod.rs` | `Seat`, `Live`, launch and verification, sessions, cancel, stop, `Runner` |
| `turn.rs` | One `session/prompt`: progress, permission answers, reply and usage; a result without a `stopReason` is a broken protocol |
