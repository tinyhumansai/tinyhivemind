# Pipes

Reads every pipe of a child to its end whatever the caller does, and keeps a
bounded part of it.

| File | Purpose |
| --- | --- |
| `mod.rs` | `JsonLines` and its halves, `Limits`, `LineError`, and the log `Tail`. |
| `test.rs` | Framing, bounds, loss and cancellation over in-memory pipes. |

A write is queued whole and performed elsewhere, so dropping a send cannot leave
half a line. A stream that outruns its reader ends in `LineError::Overflow`.
