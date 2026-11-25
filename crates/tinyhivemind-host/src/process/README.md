# Process supervision

One long-lived child, its explicit environment, private directories and drained
pipes, and a stop that shows the whole tree gone or fails. The guarantees, and
what can escape them, are in the [module documentation](mod.rs).

| Path | Purpose |
| --- | --- |
| `mod.rs` | Guarantees, limits and the public surface. |
| `instance.rs` | `Instance`: start, readiness, exit, stop and drop. |
| `supervisor.rs` | The thread that observes the child and stops its tree. |
| `scope.rs` | `containment`, the unreaped child, and which processes stopping covers. |
| [`tree/`](tree/README.md) | Linux: attribution over `/proc`, and pidfd signals. |
| [`pipe/`](pipe/README.md) | JSON lines and bounded logs. |
| `directory.rs` | `InstanceDir`: the private state and temporary directories, their lock, and what a crashed host left. |
| `types.rs` | `Launch`, `StopPolicy`, `Exit`, `Containment`. |
| `error.rs` | `Error` and `Result`. |
| [`test/`](test/README.md) | Real child processes. |

Constraints:

- Unix only. `Instance::start` runs within a Tokio runtime; stopping and
  dropping do not need one to finish.
- Each instance has one thread, which wakes every 20 ms and, on Linux, reads its
  own tree's `/proc` entries every 100 ms. The whole table is read only to stop.
- Scans of one host take turns, as the kernel serializes them anyway. One that
  has not settled gives up its turn after 500 ms and starts again.
- SIGTERM goes to every member at once, as systemd does, not to the child alone.
- The child is never reaped before its tree is proven gone: its zombie is what
  reserves its pid and group id.
