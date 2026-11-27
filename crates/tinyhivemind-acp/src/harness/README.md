# Harness

`AcpHarness`: the host's `Harness` port for any `Agent`, one process per seat. A seat id is held from the start of its start to the end of its stop: a dropped start stops what it began before it frees the id, and a failed stop keeps it, so two processes never run for one id.

| File | Purpose |
| --- | --- |
| `mod.rs` | `AcpHarness`, `Options` |
