# Gateways

An `OpenClaw` seat's bridge takes its tools from a Gateway configured with the
seat's tool server before it starts. A seat of `deployment` isolation, the
kind's default, joins the one Gateway every such seat shares, which has one
model provider, so those seats must agree on their model endpoint and name; a
seat of `seat` isolation gets a Gateway of its own. Each Gateway gets a free
loopback port and a random token.

| File | Purpose |
| --- | --- |
| `mod.rs` | Which Gateway a seat belongs to, and the plans for a deployment |
| `test.rs` | Grouping by isolation, a shared Gateway refusing a second model, tokens and ports |
