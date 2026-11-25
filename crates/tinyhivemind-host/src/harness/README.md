# The harness port

What a harness client implements, and what every client shares. Nothing here
names a client's types or talks to a harness.

| Path | Purpose |
| --- | --- |
| `mod.rs` | `Harness`, its futures and the public surface. `HarnessKind` is the language's. |
| `plan.rs` | `SeatPlan` around the package's `LoweredSeat`, and `ToolEndpoint`. |
| `seat_runner.rs` | `SeatRunner` over a `CancellableRunner`. |
| `verification.rs` | `Expected`, `Observed`, `Started` and `Mismatch`: pid, version, effective settings and tools. |
| `events.rs` | `Event`, `Progress`, `TurnUsage` and the approval port. |
| `error.rs` | `Error` and `Result`. |
| [`conformance/`](conformance/README.md) | The suite every client must pass. |
| [`test/`](test/README.md) | Payloads, verification and the seat rules. |

A client's runner is a `CancellableRunner`, returned inside a `SeatRunner`. The
wrapper does not repeat the coordinator's reservation; its documentation says
what it adds and why.
