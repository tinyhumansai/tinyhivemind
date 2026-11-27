# 38. ACP agents share one client

- **Status:** Proposed
- **Date:** 2026-11-26
- **Specification:** [harnesses](../specs/harnesses.md)
- **Evidence:** [the ACP harness runs](../experiments/2026-11-26-acp-harnesses.md)
- **Amends:** [ADR 0035](0035-a-harness-is-a-process.md), whose plan gave each
  kind a client crate of its own

## Context

ADR 0035 planned one client crate per harness kind, each speaking its
harness's native protocol: Codex's app-server, OpenClaw's Gateway WebSocket.
Each would be thousands of lines, and every further harness another crate.

Claude Code, Codex and OpenClaw all speak the Agent Client Protocol over stdio,
Claude Code and Codex through adapters published by the protocol's maintainers
and OpenClaw through a bridge it ships. Driven by one minimal ACP client
against the spike's scripted model, each called the hive tool as the right
seat, cancelled, and resumed after a restart, and a restarted agent made no
model call on its own, once one
setting per agent was made: Claude Code's built-in tools and tool search off,
and Codex out of its model-reviewed mode.

## Decision

- `tinyhivemind-acp` implements the harness port once, for any agent that
  speaks ACP version 1 over stdio. It links no agent.
- What differs between agents is an `Agent` descriptor of data: the command,
  environment and files, the `session/new` metadata, and the session mode.
  Claude Code, Codex and OpenClaw have descriptors. OpenClaw's bridge takes
  the host's tools from a shared Gateway, which the crate configures with
  every seat's server and supervises beside the bridges, and opens its
  sessions with none.
- A permission request for a tool of the host's own MCP server is allowed by
  the client, since the tool server already judges hive tools. Any other goes
  to the seat's approver.
- OpenHuman keeps its own client (`tinyhivemind-openhuman-process`) until it
  speaks ACP.

## Consequences

- A new ACP agent is a descriptor and a conformance run, not a crate.
- ACP shows no tool list, so the start-up check cannot observe what the model
  sees; each descriptor's settings hold it, and the live tests prove it.
- The adapters are extra processes: a Codex seat costs about twice its native
  app-server's memory.
- Per-turn usage depends on the agent reporting it: Claude Code and Codex do,
  OpenClaw's bridge does not.
- This stays proposed until the conformance suite passes on each pinned agent.
