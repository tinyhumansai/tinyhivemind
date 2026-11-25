# Process tests

Real shell children. Conditions are polled for with a deadline, never slept for.

| File | Purpose |
| --- | --- |
| `mod.rs` | Module list. |
| `support.rs` | Fixtures, reported pids and polling. |
| `directory.rs` | Private directories. |
| `launching.rs` | Environment, directories, pipes, readiness and exit. |
| `stopping.rs` | Linux: stopping the whole tree and nothing else. |
| `scope.rs` | The unreaped child, and process-group containment. |
| `dropping.rs` | What a drop guarantees. |
