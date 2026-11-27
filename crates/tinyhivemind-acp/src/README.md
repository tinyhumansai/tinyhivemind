# Source

| Path | Purpose |
| --- | --- |
| `lib.rs` | Crate overview and public surface |
| [`agent/`](agent/README.md) | `Agent` descriptors, their templates, and the Codex model catalog |
| [`openclaw/`](openclaw/README.md) | `Agent::openclaw`, and the Gateway its seats share: its config and its supervised process |
| [`capability/`](capability/README.md) | The one model route a seat may take on an agent |
| [`connection/`](connection/README.md) | JSON-RPC over the supervised process's JSON lines |
| [`protocol/`](protocol/README.md) | The ACP messages a seat needs, built and read as JSON |
| [`seat/`](seat/README.md) | One seat: process, session, turns, cancel and stop |
| [`harness/`](harness/README.md) | `AcpHarness`, the host's harness port, and `Options` |
| [`error/`](error/README.md) | `Error` and how the port sees it |
