# Seats

`ProcessSeats::start` checks every seat against the host's installs before
anything starts, then serves every seat's tools from one tool server, starts
the `OpenClaw` Gateways, and starts and registers each seat. A failure after
the check stops whatever had started. `stop` stops every seat, then the
Gateways, then the tool server, and reports the first failure.

| File | Purpose |
| --- | --- |
| `mod.rs` | `ProcessSeat`, `Deployment`, `ProcessSeats` |
| `test.rs` | Refusals before anything starts, cleanup after a failed start, and stopping |
