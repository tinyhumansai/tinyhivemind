# Seats

`ProcessSeats::start` checks every seat before anything starts (the host's
installs, and the same capability check its harness's client makes) then
serves every seat's tools from one tool server, starts the `OpenClaw`
Gateways, and starts and registers each seat. A failure after the check
stops whatever had started and unregisters every seat it had registered, so
the same seats can be started again on the same coordinator. `stop` stops
every seat, then the Gateways, then the tool server, and reports the first
failure; a seat is forgotten only once it has stopped, so a second `stop`
retries one that failed. Dropping `ProcessSeats` does not stop a seat the
coordinator still holds.

The deployment's directory and every seat's are made private (0700), with any
parent they need, whatever the umask: they hold the agents' state, and
`OpenClaw` refuses a temporary directory under a group- or world-writable one.
Directories above the deployment's are the host's: `OpenClaw` warns about a
group-writable one and falls back to its state directory, which is private.

| File | Purpose |
| --- | --- |
| `mod.rs` | `ProcessSeat`, `Deployment`, `ProcessSeats` |
| `test.rs` | Refusals before anything starts, cleanup after a failed start, private directories, and stopping, again after a failed stop |
