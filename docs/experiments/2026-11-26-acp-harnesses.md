# Can Claude Code, Codex and OpenClaw run as hive seats over ACP?

- **Date:** 2026-11-26
- **Status:** Recorded
- **Spec:** [harnesses](../specs/harnesses.md)
- **Follows:** [the process harness spike](2026-11-25-process-harnesses.md)
- **Code:** [the companion directory](2026-11-26-acp-harnesses/README.md)

## Question and gate

Written before the runs: can one generic Agent Client Protocol (ACP) client,
over stdio, drive each harness as a seat? For each, it must list and call the
hive tool as the right seat over HTTP MCP with a bearer header, report per-turn
token usage, cancel a turn, keep a session across a restart, and leave nothing
running after a kill. If it can, one ACP client replaces three bespoke clients.

## Method

All runs used the scripted model and MCP stub of the 2026-11-25 spike, with
Anthropic's Messages API added for Claude Code. `acp.mjs` is a minimal ACP
client: newline-delimited JSON-RPC, protocol version 1, no file system or
terminal offered to the agent. Each permission request was answered with the
first "allow" option. No real model and no credential were involved: the
fixture ignores keys.

Versions, on Linux x86_64 (all-in, load average around 80) with Node 24.16.0:

| Harness | ACP entry point | Harness inside |
| --- | --- | --- |
| Claude Code | `@agentclientprotocol/claude-agent-acp` 0.112.0 | Claude Agent SDK 0.3.331, Claude Code 2.1.331 |
| Codex | `@agentclientprotocol/codex-acp` 2.12.1 | `@openai/codex` 0.172.0 (`app-server`) |
| OpenClaw | `openclaw acp` (built-in bridge) | `openclaw` 2026.10.33 Gateway |

## Result: the gate passes for all three, except usage on OpenClaw

| | Claude Code | Codex | OpenClaw |
| --- | --- | --- | --- |
| Hive tool called as seat `a` | yes | yes | yes |
| Tools the model saw | the hive tool only | the hive tool and three MCP resource readers | the hive tool only |
| Per-turn usage in the `session/prompt` result | yes | yes | **no**: only `usage_update` with context size |
| Approval of the hive tool | none asked (allowlisted) | `session/request_permission` to the client | none asked (Gateway allowlist) |
| Cancel: answered / model call closed | 3 ms / 5 ms | 154 ms / 5 ms | 4 ms / 21 ms |
| Same session after restart (`session/load`) | yes | yes | yes |
| Processes left after `kill -9` of the agent | 1 (the CLI child) | 0 | 0 (bridge only killed) |
| Turn replayed by a new agent within 15 s | no | no | no |
| Processes per seat | 2 | 2 | 2, plus the shared Gateway's 2 |
| Idle PSS (10 s) | 292 MB | 257 to 267 MB | 1,981 MB with the Gateway |
| PSS after 13 turns | 341 MB | 258 to 282 MB | 2,163 MB with the Gateway |
| Start to `initialize` | 0.26 to 0.30 s | 0.29 to 2.6 s | 5.0 s, after 16.5 s for the Gateway |
| `session/new` | 0.3 to 2.5 s | 0.04 to 0.11 s | 0.38 s |
| Median turn, plain / one tool call | 22 / 43 ms | 20 / 36 ms | 406 / 317 ms |

Single runs on a loaded shared machine: the timings are indicative, and the
start-up times varied by an order of magnitude between runs.

## What each harness needed

1. **Claude Code hid the hive tool by default.** With the adapter's defaults the
   model was offered the built-in tools and `ToolSearch`, and the MCP tool was
   deferred behind it, so the scripted model never saw it. Passing Claude Agent
   SDK options on `session/new`, `_meta.claudeCode.options` = `tools: []`,
   `settingSources: []` and `allowedTools: ["mcp__hive__hivemind_post"]`, with
   `ENABLE_TOOL_SEARCH=false`, left exactly the hive tool and no prompt.
2. **Codex reviewed tool calls with a model.** The adapter's default mode,
   "Auto review", runs a guardian sub-turn with model `codex-auto-review`
   before each tool call. Against the fixture it did not approve, so the call
   never reached the MCP server and each tool turn took about 700 ms. Switching
   the session to the `read-only` mode (`session/set_mode`) sends the approval
   to the client instead, where the host's approver answers it. Codex also
   needs the spike's direct-tool model catalog, with an entry for the
   configured model, or it prints a metadata warning into the reply.
3. **OpenClaw takes MCP servers from the Gateway, not the session.** The bridge
   rejects per-session MCP servers, so the seat's server and bearer go into the
   Gateway config per agent, as in the spike. The bridge reports no per-turn
   token usage.

## What this changes

- One ACP client can drive all three. What differs per harness is data: the
  command, environment, `session/new` metadata, mode, and where MCP is
  configured. That is a descriptor, not a crate.
- ACP adds an adapter process per seat for Claude Code and Codex. Against the
  spike's native Codex `app-server` (127 MB RSS idle) the adapter roughly
  doubles a Codex seat's memory.
- OpenClaw remains the outlier: about 2 GB for the Gateway and bridge, a 20 s
  cold start, and no usage. It stays shared per deployment, and its usage needs
  another source (the Gateway's own frames, as in the spike) or the row stays
  rejected.
- A Claude Code CLI child outlived `kill -9` of its adapter. The host's
  supervisor already stops the whole tree; this confirms it must.

## Not tested

Real models and credentials; the Gateway's own crash and its turn replay (only
the bridge was killed here); concurrent seats; long sessions and compaction;
ACP protocol v2; whether each adapter honours `session/cancel` during a tool
call rather than a model call; macOS and ARM.
