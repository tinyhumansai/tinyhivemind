# tinyhivemind-acp

Hive seats on any [Agent Client Protocol](https://agentclientprotocol.com)
agent, each a supervised child process spoken to over stdio. One client for
every agent: what differs between them is an `Agent` descriptor of data.
Claude Code and Codex are the two measured; see
[the ACP harness runs](../../docs/experiments/2026-11-26-acp-harnesses.md).
It links no agent ([ADR 0035](../../docs/adr/0035-a-harness-is-a-process.md)).

| Path | Purpose |
| --- | --- |
| [`src/`](src/README.md) | The crate |
| [`tests/`](tests/README.md) | Live hive tests on the pinned agents |
