# Process tests

Real shell children. A condition is polled for with a deadline. The only plain
sleeps look for something that must not happen, or are doubled until it does.

| File | Purpose |
| --- | --- |
| `mod.rs` | Module list. |
| `support.rs` | Fixtures, reported pids and polling. |
| `directory.rs` | Private directories. |
| `launching.rs` | Environment, directories, pipes, readiness and exit. |
| `stopping.rs` | Linux: stopping the whole tree and nothing else. |
| `scope.rs` | The unreaped child, and process-group containment. |
| `dropping.rs` | What a drop guarantees. |
