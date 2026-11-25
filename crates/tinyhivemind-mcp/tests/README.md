# Tests

Every test drives a real server over a loopback HTTP/1.1 client.

| File | Covers |
| --- | --- |
| `http.rs` | Bearer, Host, Origin, body bound, methods and paths, version negotiation, notifications, malformed messages, tools-only surface, redacted `Debug`, the connection bound, shutdown |
| `calls.rs` | Hive tools: the served schemas, turn and call checks, retries, reuse, another seat, a forged actor, sends, joins, `create_agent`, a hook-denied call |
| `programs.rs` | Executable tools: retries, an unknown outcome, hooked reads, hook denial, the `read_only` and `supervised` gates, the approver |
| `seats.rs` | `SeatTools::from_lowered` refusals and access, permission tool names, duplicates |
| `e2e.rs` | A harness's whole session: initialize, notification, GET, list, and a post as seat `a`, refused as seat `b` |
| [`support/`](support/README.md) | The client and the held-turn hive fixture |
| [`fixtures/`](fixtures/README.md) | Scripts the executable tools and hooks run |
