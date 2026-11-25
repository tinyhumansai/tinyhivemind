# Harnesses

Status: Accepted. Implementation: [plan](../plans/2026-11-25-process-harnesses.md).
Boundary: [ADR 0035](../adr/0035-a-harness-is-a-process.md),
[ADR 0036](../adr/0036-hive-tools-are-served-over-loopback-mcp.md).
Evidence: [the harness spike](../experiments/2026-11-25-process-harnesses.md).

Nothing here is implemented yet. Until the plan's removal phase lands, the
linked OpenHuman adapter remains the way a package is deployed.

## Problem

Every seat runs on OpenHuman, linked as a Rust library. That ties a consumer to
one OpenHuman revision, a recursive submodule and a copied set of `[patch]`
tables, and it rules out agents that are not Rust. OpenClaw publishes no
embeddable agent runtime, and Codex is a command-line program.

## Goals

- A seat runs on OpenHuman, OpenClaw or Codex, each as a supervised child
  process. One hive may mix them.
- Seats talk only through the coordinator, whichever harness they run on.
- The package names the harness. A setting the harness cannot honour fails
  `check`; a permission is never accepted and left unenforced.
- No harness is linked by any crate.

## Non-goals

- Feature parity between harnesses. Each has a capability set (below).
- Downloading or updating harness binaries from library code.
- Isolation between mutually hostile seats under one OS user. A process is not
  a security boundary against another process of the same user.

## Vocabulary

- **Harness kind:** `openhuman`, `openclaw` or `codex`.
- **Instance:** one supervised process tree and its private state directory.
- **Turn:** one coordinator reservation, identified by `TurnRequest.turn_id`.

| Kind | Surface | Seats per instance |
| --- | --- | --- |
| `openhuman` | `openhuman-core run --headless-api`: JSON-RPC, SSE | one |
| `openclaw` | `openclaw gateway`: WebSocket protocol 4 | one deployment, or one seat if `isolation` is `seat` |
| `codex` | `codex app-server`: JSON-RPC over stdio | one |

OpenHuman needs one instance per seat because `web_chat` takes its agent from
process configuration. Codex does not need it for tool identity; it is the
default for crash isolation and separate state. OpenClaw shares because an
instance costs gigabytes. A shared instance holds the credentials of every seat
in it, and opens an MCP session for each of them on every run.

## The package

```json
{
  "harnesses": [
    { "id": "claw", "kind": "openclaw", "isolation": "deployment" },
    { "id": "codex", "kind": "codex" }
  ],
  "seats": [
    { "id": "lead", "role": "lead" },
    { "id": "scout", "role": "research", "harness": "claw" },
    { "id": "builder", "role": "implement", "harness": "codex" }
  ]
}
```

- `harnesses` is an optional list. Like every section, each item has an `id`;
  ids are unique. `isolation` is `seat` or `deployment` and only `openclaw`
  accepts `deployment`, which is its default.
- `harness` is optional on `runtime`, a profile and a seat, and resolves in that
  order with the most specific value winning, as `model` does. Absent everywhere
  means an OpenHuman seat.
- A reference to an undeclared harness is a `check` error. A profile used for
  delegation may not name one.
- A package never names a harness executable, port, state path or harness
  config. The host maps a kind to a program. A package may already declare
  executable tools; this rule keeps the process that *enforces* permissions out
  of reach of a package edit.
- An absent field is not serialized, so an existing package keeps its content
  id. A test pins one.
- `harnesses` and `runtime.harness` are in the `integration` edit class. A
  change to a seat's `harness` requires `integration` as well as `roster`, so a
  hive allowed to edit its roster cannot move a seat to another harness.
- Packages deployed together must agree on a shared seat's harness and on any
  harness declared under the same id.

## Capabilities

`tinyhivemind-lang` holds one capability table per kind. `check` reports a field
the resolved harness cannot honour under the existing `integration` rule, at the
field's location.

| Field | openhuman | openclaw | codex |
| --- | --- | --- | --- |
| prompt, model, hive and executable tools, limits.timeout_ms | yes | yes | yes |
| hooks and permissions on hive and executable tools | yes, host side | yes, host side | yes, host side |
| built-in tool groups | exact allowlist | off | off |
| `tool_rules` on built-in tools | rejected until a release has them | rejected | rejected |
| hooks on built-in tools | rejected | rejected | rejected |
| package MCP servers | through the host server | through the host server | through the host server |
| `session_store = "host"`, `memory_engine = "host"` | rejected | rejected | rejected |
| subagents, `definition_base`, `jev`, `session` tuning | rejected at first | rejected | rejected |
| workflows | coordinator schedule | coordinator schedule | coordinator schedule |

Built-in tools start **off** for every kind. Turning one on is a later change
that must first show its permissions are enforced. "Rejected" is the starting
point, not a promise: a row moves when a conformance test proves the setting.

Two consequences follow. A package that relies on OpenHuman's built-in tools
with hooks or argument rules cannot move to the process harness yet, and that
blocks removal of the linked adapter for it. And "workflows" and proxied package
MCP servers are new host work, not something the spike exercised.

## The harness port

`tinyhivemind-host` owns a port that names no harness:

```rust
pub trait Harness: Send + Sync {
    fn kind(&self) -> HarnessKind;
    fn start(&self, seat: SeatPlan, tools: ToolEndpoint) -> StartFuture; // -> Arc<dyn AgentRunner>
}
```

`AgentRunner`, `TurnRequest` and `TurnOutcome` in `tinyhivemind-hives` are
unchanged. Process runners register under the coordinator's `runtime_id`.

### Every runner

1. Runs at most one turn per seat at a time. All three harnesses do something
   other than refuse a second message: OpenHuman interrupts, Codex steers,
   OpenClaw queues.
2. Returns the same session id for a seat's life. A failed resume is an error
   and never a new session.
3. Cancels the harness turn when its future is dropped, then proves it stopped
   or kills the instance.
4. Never resubmits a turn whose outcome is uncertain. No harness has a durable
   idempotency key. The coordinator's interrupted state is the only recovery.
5. Reports usage for the turn. Codex reports a thread total, so its usage is a
   difference of totals.
6. Verifies the instance after start: the process answering is the child it
   started, its version is the pinned one, its effective config is the one
   written, and the model-visible tool list is exactly the expected set. Any
   mismatch fails the start.
7. Starts the child with a cleared environment plus an explicit list, a private
   state directory and temp directory, and no operator token visible to tools.

### Supervision

Stop is: cancel, close event streams, SIGTERM, bounded wait, SIGKILL. Killing a
process group is not enough. OpenClaw's broker has its own session, and Codex
shell children outlived both interrupt and kill. Linux supervision must cover
the whole tree (a cgroup or a subreaper); where it cannot, the runner refuses a
configuration that spawns such children.

## Hive tools

`tinyhivemind-mcp` serves the hive vocabulary and the package's executable tools
over Streamable HTTP on a random port of 127.0.0.1.

- **Identity.** Each seat has its own random bearer, held in memory and in the
  instance's private config. The bearer alone decides the actor.
- **Turn.** Every call that changes state carries a `turn` argument that must
  equal the seat's running `turn_id`. A call with no running turn, or naming
  another turn, is refused. The bearer cannot do this alone: OpenClaw resumes
  an interrupted turn after a crash, and its calls would carry a valid bearer
  while a newer turn is running. `turn` is added to the served schema only.
- **Effects.** A call that reached the coordinator is recorded against its turn,
  so a repeated call with the same identity does not act twice.
- **Surface.** Only tools. Resource methods return an empty list, since Codex
  always offers resource readers. A bad `Host` or any `Origin` is refused, no
  redirect is ever sent, and request bodies are bounded.
- **Policy.** Package hooks, permission layers and send policy run here, before
  dispatch, for every tool this server serves.

`turn` protects against stale calls, not against a hostile model: the model can
read it. A seat acting within its own current turn has its own authority anyway.

## Per-kind rules

**OpenHuman.** Exact `tool_allowlist`; never `"*"` with a denylist, because
`mcp_call_tool` bypassed a deny and posted as another seat. Read the hashed tool
name from `agent_registry_available_tools`, which also waits out the first-turn
MCP warm-up. Match `/health.pid` to the child. Compare `config_get` with what was
written, since bad TOML silently resets to defaults. Use `web_chat`, never
`agent_chat`, which bypasses approvals. Disable suggestions, memory, search,
hooks and any hosted provider route.

The released binary stops a custom agent after eight model iterations and
reports it as a normal end. Until that is configurable, a package asking for more
iterations is rejected for this kind, and a turn that reaches the cap is reported
as failed. The runner counts `iteration_start` events to tell.

**OpenClaw.** Tool search and code mode off; `tools.allow` exactly the seat's
server; the deny groups recorded in the experiment; agent-to-agent off. After a
crash, do not admit a turn for a seat until its recovery is known to be settled.
The `agent:<seat>:subagent:<thread>` key skipped recovery in the spike but relies
on internals, so it is defence in depth and the `turn` check is the guarantee.
Retry `UNAVAILABLE` after start. Set `logging.file`, temp directory and
`catalogRefresh.enabled = false`.

**Codex.** A pinned model catalog with direct tools. Without one, known models
hide MCP tools or route them through a JavaScript tool. `CODEX_HOME` is private;
the user's own is never read. MCP config and bearer are sent on both
`thread/start` and `thread/resume`, with `required = true`. Shell, multi-agent,
goals, plugins and apps are off. The working directory is not a git checkout.
Login tokens are a host secret and enter only the instance's environment.

## Pins

A lock file records version, target and sha256 for each kind. A script fetches
and verifies; the library takes a path. Live tests run against exactly the pin,
with outbound network blocked except to the model endpoint under test.

## Acceptance

One conformance suite runs over the port, against a scripted fake process and
each kind: hive tool with the right actor; wrong bearer; another seat's bearer;
stale `turn`; second turn continues; resume after restart; resume failure;
timeout; drop stops the model call; kill mid-turn neither replays nor loses the
seat; per-turn usage; exact model-visible tool list; no process left after stop.

Live tests (`live_*`, opt-in) run the same cases on the pinned binaries, then a
mixed hive reaches a decision with every message in the coordinator transcript.
