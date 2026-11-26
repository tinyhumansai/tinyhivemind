# Source

| Path | Purpose |
| --- | --- |
| `lib.rs` | Crate overview, example and public surface |
| `error/` | `Error`: why a server could not start or a seat could not be served; `test.rs` pins the messages |
| [`server/`](server/README.md) | The listener, request admission and JSON-RPC |
| [`seat/`](seat/README.md) | `SeatTools` and the seat's bearer |
| [`tools/`](tools/README.md) | The served catalog and how one call is carried out |

The public surface is `HiveToolServer`, `Services`, `ServerOptions`,
`SeatTools`, `PROTOCOL_VERSIONS` and `Error`. A seat's endpoint is the host's
`tinyhivemind_host::harness::ToolEndpoint`, and the approver is the host's
`tinyhivemind_host::harness::Approver`.
