# tinyhivemind-acp

Hive seats on any [Agent Client Protocol](https://agentclientprotocol.com)
agent, each a supervised child process spoken to over stdio. One client for
every agent: what differs between them is an `Agent` descriptor of data.
Claude Code, Codex and OpenClaw are the three measured; see
[the ACP harness runs](../../docs/experiments/2026-11-26-acp-harnesses.md).
OpenClaw's seats share a Gateway, which the crate configures and supervises.
It links no agent ([ADR 0035](../../docs/adr/0035-a-harness-is-a-process.md)).

An agent's own tools stay off, as [the harness spec](../../docs/specs/harnesses.md)
requires for every kind. A seat that edits files does it through the host's
executable tools, which run in the seat's workspace under the host's access
gate; the live hive tests in
[`tinyhivemind-deploy`](../tinyhivemind-deploy/tests/README.md) show every
harness editing that way and a read-only seat refused.

| Path | Purpose |
| --- | --- |
| [`src/`](src/README.md) | The crate |
