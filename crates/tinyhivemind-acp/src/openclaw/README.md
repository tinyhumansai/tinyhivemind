# OpenClaw

OpenClaw's ACP bridge (`openclaw acp`) refuses MCP servers per session, so a
seat's tool server goes into the config of the Gateway the bridge connects
to: one Gateway agent per seat, with that seat's server and bearer, allowed
that server's tools and denied every built-in group. The bridge opens the
seat's session on that agent (`--session agent:{seat}:acp`) with no servers of
its own, and reads the Gateway token from a file rather than its arguments.

Every seat is in the plan when the Gateway starts; a seat added later needs a
new Gateway. The bridge reports no per-turn usage. About 2 GB and a cold start
of tens of seconds, the Gateway is shared by a deployment's seats
(`docs/experiments/2026-11-26-acp-harnesses.md`).

The Gateway's directories are made private (0700) whatever the umask: OpenClaw
refuses a temporary directory under a group- or world-writable one, and its
seats then never reach the model.

| File | Purpose |
| --- | --- |
| `mod.rs` | `GatewayPlan` and its config, `Gateway` (a supervised `Instance`, ready once `GET /readyz` answers 200, not merely once it listens), `Agent::openclaw` |
| `test.rs` | The config per seat, the hidden token, the bridge's arguments, the readiness probe, and a Gateway that never listens |
