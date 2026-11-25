# The fake openhuman-core

| File | Purpose |
| --- | --- |
| `surface.rs` | The scripted RPC, `/health` and `/events` surface, recorded from 0.64.15; driven by a control directory. Also compiled into the unit tests. |
| `child.rs` | The `tinyhivemind-fake-openhuman` binary: the surface on `--port`, with the runner's environment, and a grandchild that escapes its session on request. |
| `subject.rs` | Both as a conformance `Subject`, launched through a wrapper script that sets `CONTROL`. |
