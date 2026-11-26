# Connection

JSON-RPC 2.0 over the host's supervised JSON lines. Responses settle calls; notifications and requests go to the running turn's route, or a request is refused at once.

| File | Purpose |
| --- | --- |
| `mod.rs` | `Connection`, `Incoming`, `Error` |
| `test.rs` | Calls, routes and refusals against an echoing child |
