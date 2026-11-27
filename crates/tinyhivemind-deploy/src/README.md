# Source

| Path | Purpose |
| --- | --- |
| `lib.rs` | Crate overview and public surface |
| [`seats/`](seats/README.md) | `ProcessSeats`, `ProcessSeat` and `Deployment`: check, serve, start the Gateways, start and register each seat; stop |
| [`installs/`](installs/README.md) | `Installs` and `Pinned`: where each kind's pinned install is, and the harness a seat starts on |
| [`gateway/`](gateway/README.md) | The `OpenClaw` Gateways a deployment's seats need, grouped by isolation |
| [`error/`](error/README.md) | `Error` and `Result` |
| `fixture.rs` | Seats for the unit tests, on a given kind, isolation and model |
