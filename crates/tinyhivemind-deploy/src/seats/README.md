# Seats

`ProcessSeats::start` checks every seat against the host's installs before
anything starts, then serves every seat's tools from one tool server, starts
the `OpenClaw` Gateways, and starts and registers each seat. A failure after
the check stops whatever had started. `stop` stops every seat, then the
Gateways, then the tool server, and reports the first failure.

The deployment's directory and every seat's are made private (0700), with any
parent they need, whatever the umask: they hold the agents' state, and
`OpenClaw` refuses a temporary directory under a group- or world-writable one.
Directories above the deployment's are the host's: `OpenClaw` warns about a
group-writable one and falls back to its state directory, which is private.

| File | Purpose |
| --- | --- |
| `mod.rs` | `ProcessSeat`, `Deployment`, `ProcessSeats` |
| `test.rs` | Refusals before anything starts, cleanup after a failed start, private directories, and stopping |
