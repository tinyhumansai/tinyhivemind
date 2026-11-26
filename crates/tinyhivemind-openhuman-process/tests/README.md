# Tests

| Path | Purpose |
| --- | --- |
| `conformance.rs` | The host's conformance suite over this runner and the fake child; refusals through the port. Needs `--features conformance`. |
| `live.rs` | `live_*`: the suite and the spike gate on the pinned binary, opted into with `THM_OPENHUMAN_CORE` (and Node, from `THM_NODE` or `PATH`). |
| `hive.rs` | `live_hive_*`: two seats on the pinned binary in one hive, with a real coordinator and a real `tinyhivemind-mcp` server; each posts once though it retries, completes, and the episode settles. Opted into with `THM_OPENHUMAN_CORE`. |
| [`fake/`](fake/README.md) | The fake `openhuman-core` surface and child |
| [`hive/`](hive/README.md) | The tool-calling model the hive test plays |
| [`live/`](live/README.md) | The live subject and scripted model |
| [`support/`](support/README.md) | Shared HTTP server and seat helpers |
