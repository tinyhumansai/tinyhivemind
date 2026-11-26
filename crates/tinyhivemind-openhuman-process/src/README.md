# Source

| Path | Purpose |
| --- | --- |
| `lib.rs` | Crate overview and public surface: `OpenHumanHarness`, `Program`, `Options`, `capability`. |
| [`error/`](error/README.md) | The crate's internal `Error`, and how it reaches the port as `harness::Error`. |
| `fixture.rs` | Unit tests only: the in-process fake surface with a written config, and a client. |
| [`capability/`](capability/README.md) | What a lowered seat may ask of this harness: `check`, `Refusal`, `SeatModel`. |
| [`config/`](config/README.md) | The generated `config.toml`, and comparing an effective config with it. |
| [`rpc/`](rpc/README.md) | The loopback client: JSON-RPC, `/health`, `/events`. |
| [`turn/`](turn/README.md) | One turn over `channel_web_chat` and its events; cancellation. |
| [`instance/`](instance/README.md) | Launching an instance, tool discovery, start-up verification. |
| [`harness/`](harness/README.md) | `OpenHumanHarness` and the seat it runs. |
