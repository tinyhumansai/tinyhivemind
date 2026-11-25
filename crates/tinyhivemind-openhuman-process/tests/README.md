# Tests

| Path | Purpose |
| --- | --- |
| `conformance.rs` | The host's conformance suite over this runner and the fake child; refusals through the port. Needs `--features conformance`. |
| `live.rs` | `live_*`: the suite and the spike gate on the pinned binary, opted into with `THM_OPENHUMAN_CORE` (and Node, from `THM_NODE` or `PATH`). |
| [`fake/`](fake/README.md) | The fake `openhuman-core` surface and child |
| [`live/`](live/README.md) | The live subject and scripted model |
| [`support/`](support/README.md) | Shared HTTP server and seat helpers |
