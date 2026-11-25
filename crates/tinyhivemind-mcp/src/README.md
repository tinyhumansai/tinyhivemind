# Source

| Path | Purpose |
| --- | --- |
| `lib.rs` | Crate overview, example and public surface |
| `error.rs` | `Error`: why a server could not start or a seat could not be served |
| [`server/`](server/README.md) | The listener, request admission and JSON-RPC |
| [`seat/`](seat/README.md) | `SeatTools` and the seat's bearer |
| [`tools/`](tools/README.md) | The served catalog and how one call is carried out |

The public surface is `HiveToolServer`, `Services`, `ServerOptions`,
`SeatTools`, `PROTOCOL_VERSIONS` and `Error`. A seat's endpoint is the host's
`tinyhivemind_host::harness::ToolEndpoint`, and the approver is the host's
`tinyhivemind_host::harness::Approver`.
