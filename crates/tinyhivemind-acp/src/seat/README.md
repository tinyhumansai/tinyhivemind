# Seat

One seat: its agent process in a private directory, its one session (new, or loaded after a restart), the turn it runs, cancel and stop.

| File | Purpose |
| --- | --- |
| `mod.rs` | `Seat`, `Live`, launch and verification, sessions, cancel, stop, `Runner` |
| `turn.rs` | One `session/prompt`: progress, permission answers, reply and usage |
