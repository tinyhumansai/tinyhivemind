# Connection

JSON-RPC 2.0 over the host's supervised JSON lines. Responses settle calls; notifications and requests go to the running turn's route, or a request is refused at once. `answer_detached` answers from a task, for a `Drop`.

| File | Purpose |
| --- | --- |
| `mod.rs` | `Connection`, `Incoming`, `Error` |
| `test.rs` | Calls, routes, refusals and detached answers against an echoing child, each wait bounded |
