# Hive language integrations

Status: Accepted. Implementation: [plan](../plans/2026-10-23-hive-language-integrations.md).
Boundary: [ADR 0034](../adr/0034-the-hive-language-defines-the-deployment.md),
which supersedes the parallel-schema stance of
[`openhuman-hive-config.md`](openhuman-hive-config.md).
Tools: [executable tools](executable-tools.md).

## Problem

A hive package (`hive.json`, `constitution.json`, `memory.json` and its
Markdown) defines who is in a hive and how it deliberates, but not what its
agents run on. A seat's `model`, `mcp` and `skills` were bare names that no
code resolved, and everything a deployment actually needs — provider, model,
MCP servers, skills, permissions, workflows — lived in the OpenHuman adapter's
own `HiveConfig`, a second schema that a package never lowered into. Two
schemas describing one hive drift, and the one an editor could change (the
package) was the one that did nothing.

## Goals

- The package is the single definition of a hive mind: structure,
  deliberation policy, and every integration its agents run with.
- Every integration setting either lowers to OpenHuman or fails `check` with a
  located diagnostic. Nothing a package says is silently ignored.
- Tools and hooks can be executables shipped in the package and written in any
  language (see [executable tools](executable-tools.md)).
- Secrets are references only, never values.
- An editing hive cannot widen its own reach: integration sections are a
  separate edit class the constitution can forbid.

## Non-goals

- Hosts other than OpenHuman. The language names no OpenHuman type, but its
  integration fields are exactly what OpenHuman can run; a setting OpenHuman
  cannot honour is not added "for later".
- Resolving credentials, opening files, or starting processes in the pure
  crates. Lowering produces data; the adapter acts on it.
- Keeping `HiveConfig` as an input format. It is retired (ADR 0034).

## Package additions

All sections are optional and default to empty, so every existing package is
still valid and lowers to the same hive.

```text
hive.json          # gains the sections below
tools/<id>.*       # executable tool sources, carried as package files
skills/<id>/…      # skill bundles; skills/<id>/SKILL.md is required
```

`parse` now keeps, besides Markdown, every file under `tools/` and `skills/`,
so scripts take part in the package's content identity and lineage.

### Section reference

| Section | Shape | Lowers to (OpenHuman) |
| --- | --- | --- |
| `runtime` | object, below | runtime builder, native config |
| `providers` | `[{id, kind, endpoint?, credential?}]` | `Provider` |
| `models` | `[{id, provider?, name, temperature?, max_tokens?, limits}]` | `ModelDefaults`, agent model |
| `mcp` | `[{id, transport, endpoint, args, env, headers, tools?, deny_tools}]` | `McpServer` per agent |
| `skills` | `[{id}]` | skills root `<package>/skills`, per-agent selection |
| `tools` | `[ExecTool]` | native tool per seat ([executable tools](executable-tools.md)) |
| `toolpacks` | `[{id, description, tools, mcp, skills, groups}]` | tool set plus `ToolGroups` modes |
| `permissions` | `[{id, access, sandbox, allow_tools?, deny_tools, send_destinations?, send_operations?, tool_rules?}]` | permission layers |
| `profiles` | `[{id, …seat integration fields}]` | merged into each seat using it |
| `hooks` | `[{id, event, matcher?, run, fail_closed, timeout_ms?}]` | tool hook / stop hook per seat |
| `session` | `{compact_context?, max_history_messages?, tool_result_budget_bytes?, max_tool_iterations?, parallel_tools?, agents_md?}` | native `agent` config |
| `jev` | `{ranker, route, base_url?, top_k}` | native `agent.tool_search` |
| `features` | `{name: bool}` | integration switches and tool environment |
| `workflows` | `[{id, schedule, target, prompt, enabled, retries, single_flight}]` | native cron jobs |

`runtime` fields: `deployment_id?`, `workspace?`, `session_store`
(`memory` \| `sqlite` \| `host`, default `memory`), `memory_engine`
(`disabled` \| `memory` \| `host`, default `disabled`), `autonomy`
(`read_only` \| `supervised` \| `autonomous`, default `supervised`),
`sandbox` (`none` \| `read_only` \| `sandboxed`), `concurrency` (default 4),
`max_agents?`, `include_user_skills`, `start_scheduler`, and the defaults every
seat inherits: `provider?`, `model?`, `permissions?`, `toolpacks`, `limits`.

`limits` is `{timeout_ms?, iterations?, budget_usd?}` wherever it appears.

A `credential`, an MCP `env` value and an MCP `headers` value are a
**secret reference**: `{"env": "NAME"}` or `{"store": "key"}`. Any other
shape, including a bare string, is a decode error.

`provider.kind` is `openai` (any compatible endpoint), `openrouter`,
`anthropic` or `inherit`. `mcp.transport` is `stdio` or `http`; OpenHuman has
no SSE client, so `sse` is rejected rather than accepted and dropped.

`toolpacks[].groups` maps an OpenHuman tool group id to `advertised`,
`withheld` or `off`. The language checks the mode; the adapter rejects an
unknown group id at deployment.

`hooks[].event` is `pre_tool_use`, `post_tool_use` or `stop` (observe-only;
see [executable tools](executable-tools.md#hooks)). `run` names a
declared executable tool; a hook is a tool bound to an event, so client hooks
are scripts too. `matcher` limits a tool event to tool names matching it (`*`
wildcards).

`workflows[].target` is `{"seat": "<id>"}` or `{"hive": "<this hive id>"}`.
`schedule` is a five-field cron expression; the adapter validates it with
OpenHuman's scheduler.

### Seats and profiles

A seat gains `profile?`, `agent_id?`, `toolpacks`, `hooks`, `permissions?`,
`limits` and `context` (extra `context/*.md` paths composed into its prompt).
`model`, `mcp` and `skills`, which already existed, now **reference** the
`models`, `mcp` and `skills` sections by id.

A profile holds the same integration fields as a seat (`model`, `permissions`,
`toolpacks`, `tools`, `mcp`, `skills`, `hooks`, `context`, `limits`) plus
`subagents` (profile ids a seat may delegate to), `bare_prompt` and
`definition_base`.

Precedence, lowest first: `runtime` defaults, the seat's profile, the seat.
Scalars (`model`, `permissions`) take the most specific value; lists
(`toolpacks`, `mcp`, `skills`, `hooks`, `context`, `tools.deny`) concatenate
without duplicates; `limits` merge field by field; `tools.allow` takes the most
specific non-empty list. A toolpack expands into its tools, MCP servers and
skills before that merge.

### Features

`features` maps a flag name (`[a-z0-9_]+`) to a boolean. Four names switch an
integration off for the whole hive when `false`: `mcp`, `skills`,
`script_tools` and `hooks`. Every flag, including those four, is passed to each
executable tool and hook as `THM_FEATURE_<NAME>=1` or `0`, which is how a
client script branches on a flag without a harness restart.

### Several packages, one deployment

The adapter deploys one or more packages onto one OpenHuman runtime. A seat id
declared by several packages is one agent. Its integration (prompt, model,
profile, MCP, skills, tools, hooks, toolpacks, limits) must agree across the
packages, or deployment fails naming the seat; `permissions` and `context`
may differ and act as that hive's narrowing overlay, as `HiveConfig`
memberships did. `runtime`, and any provider, model, MCP server, skill, tool
or permission set declared under the same id in several packages, must be
identical.

## Validation

`check` and `diagnose` report, with the file and JSON Pointer:

- blank or duplicate identities in every new section (`identity`);
- references to undeclared providers, models, MCP servers, skills, tools,
  toolpacks, permission sets, profiles, hooks, seats and context documents
  (`reference`);
- a skill without `skills/<id>/SKILL.md`, a tool whose package-relative
  command is missing (`missing_document`);
- unsafe or misplaced paths (`path`);
- a zero `concurrency`, `timeout_ms`, `iterations`, `top_k` or
  `max_history_messages`, a negative or non-finite `budget_usd` or
  `temperature`, and a cron expression without five fields (`bound`);
- an `allow_tools` entry also in `deny_tools`, a `matcher` on a `stop` hook
  (stop has no tool), an `argv` placeholder that names no argument, and a
  feature name outside `[a-z0-9_]+` (`integration`).

Permission narrowing (a hive overlay never widens the seat) and MCP agreement
across packages are deployment checks in the adapter, because they need every
package at once.

## Editing

A new patch class, `integration`, owns `runtime`, `providers`, `models`,
`mcp`, `skills`, `tools`, `toolpacks`, `permissions`, `profiles`, `hooks`,
`session`, `jev`, `features`, `workflows`, the files under `tools/` and
`skills/`, and the integration fields of a seat. A constitution that does not
list `integration` in `permitted_edits` forbids a self-edit from changing what
its agents can run or reach. Prompts, roster and policies keep their classes.

## Acceptance criteria

1. Every package under `crates/tinyhivemind-lang/examples/hives/` parses,
   checks clean, and lowers to the same seats, desks and policies as before.
2. A package with every section lowers to a `LoweredHive` whose seats carry
   resolved provider, model, MCP, skill, tool, hook, permission and context
   values, and whose hive carries runtime, session, Jev, feature and workflow
   settings.
3. One fixture per validation rule above produces exactly that diagnostic.
4. The adapter deploys every example in `examples/openhuman/hives/` from a lang
   package; the `HiveConfig` parser is gone.
5. A script tool and a script hook run in an offline OpenHuman turn.

## Open questions

None.
