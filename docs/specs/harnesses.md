# Harnesses

Status: Accepted. Implementation: [plan](../plans/2026-11-25-process-harnesses.md).
Boundary: [ADR 0035](../adr/0035-a-harness-is-a-process.md),
[ADR 0036](../adr/0036-hive-tools-are-served-over-loopback-mcp.md).
Evidence: [the harness spike](../experiments/2026-11-25-process-harnesses.md).

Implemented so far, by step 1 of the plan: the harness-neutral host crate, the
harness port with the seat rules below, start verification, process supervision
on Linux, and a conformance suite that a scripted fake harness passes. Step 4b adds the
`openhuman` client, `crates/tinyhivemind-openhuman-process`, which passes that
suite offline and on the pinned binary. No MCP server and no `harnesses`
section exist yet, so no deployment uses it. Until the plan's removal phase
lands, the linked OpenHuman adapter remains the way a package is deployed.

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
| prompt, model name, hive and executable tools, limits.timeout_ms | yes | yes | yes |
| provider `openai` with an explicit `endpoint`, no `credential` | Chat Completions | Chat Completions | Responses API |
| provider `openai` without an `endpoint` | rejected | rejected | rejected |
| provider `openrouter`, `anthropic` | rejected | rejected | rejected |
| provider `inherit`, or no provider at all | rejected | rejected | rejected |
| provider `credential` | rejected | rejected | rejected |
| hooks and permissions on hive and executable tools | yes, host side | yes, host side | yes, host side |
| `sandbox` `none` or absent, on `runtime` and `permissions` | yes | yes | yes |
| `sandbox` `read_only` or `sandboxed` | rejected | rejected | rejected |
| `autonomy` and `permissions.access` | yes, host side | yes, host side | yes, host side |
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

A `sandbox` is rejected because executable tools on this path run from the host's
MCP server, where nothing confines them yet, and a harness's own sandbox does not
reach them. `autonomy` and `access` are judged by the MCP server before hooks and
dispatch, for executable tools not declared `side_effects = "read"` and every
proxied MCP call: `read_only` refuses them; `supervised`, the default, sends each
to the host's approver and refuses it when there is none; `autonomous` adds no gate.
Hive tools stay under send and management policy, as on the linked path. The
harness's own approval prompts are never relied on.

Two consequences follow. A package that relies on OpenHuman's built-in tools
with hooks or argument rules cannot move to the process harness yet, and that
blocks removal of the linked adapter for it. And "workflows" and proxied package
MCP servers are new host work, not something the spike exercised.

### Providers

A `model` row means nothing without the provider it runs on, so `check` judges
the resolved provider of every seat: its kind, endpoint and credential.

- **What the rows rest on.** The spike ran one shape per kind, against a scripted
  endpoint on loopback that did not check the model request's key: OpenHuman with
  `auth_style = "none"`, OpenClaw (`openai-completions`) and Codex
  (`wire_api = "responses"`) with the literal key `dummy`. Nothing else was run,
  so everything else starts rejected. A row moves when a conformance test proves
  it on the pinned binary; for `credential` that test uses an endpoint which
  refuses the call without the right key. No hosted model is usable until then.
- **Codex needs the Responses API.** `openai` means any compatible endpoint and
  the language cannot say which API one serves, so `check` cannot see this
  mismatch. The turn then fails; the runner does not retry another API.
- **No fallback.** `inherit` means "whatever provider the runtime already has". A
  child process has none, so it is rejected, as is a seat that resolves to no
  provider. A process harness never uses its own default or hosted provider. The
  generated config names one provider, and the start check (rule 6 below)
  compares the effective one with it and fails on any other usable route.
- **Credentials.** The host resolves the `SecretRef` and puts the value only in
  that instance's cleared environment or its private config: never in the
  package, a command line, the shared environment, a log or a tool's environment.
  Where a kind insists on a key and the package has no credential, the host
  writes a fixed placeholder.
- **Shared OpenClaw.** One provider was tested per Gateway. Seats in one instance
  must resolve to the same provider until a test shows more; otherwise use
  `isolation = "seat"`.

These rows bind process harnesses. The linked adapter keeps its own limits in
`crates/tinyhivemind-openhuman/src/deploy/compatibility.rs` while it lasts: a
package provider endpoint or credential conflicts with a host-injected provider,
a credential needs an endpoint or a kind, and `anthropic` needs an injected
provider. It also uses one provider per runtime, and an absent provider there
really does inherit OpenHuman's own (`deploy/runtime.rs`), which the process
harness does not copy.

## The harness port

`tinyhivemind-host` owns a port that names no harness:

```rust
pub trait Harness: Send + Sync {
    fn kind(&self) -> HarnessKind;
    fn start(&self, seat: SeatPlan, tools: ToolEndpoint) -> StartFuture; // -> Arc<dyn AgentRunner>
    fn stop(&self, seat_id: &str) -> StopFuture;
}
```

`stop` exists because a runner alone gives its host no way to await proof that
nothing is left running.

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
   state directory and temp directory, and no operator token or provider
   credential visible to tools.

### Supervision

Stop is: cancel, close event streams, SIGTERM, bounded wait, SIGKILL. Killing a
process group is not enough. OpenClaw's broker has its own session, and Codex
shell children outlived both interrupt and kill.

On Linux, stop covers the child, its process group, every process carrying the
instance's random environment marker, and their sampled descendants. It repeats
until consecutive scans find nobody and reports survivors as an error. A process
that left the group, hides the marker and was orphaned before a sample saw it
can escape, and stop then reports success. Elsewhere only the process group is
covered, and the runner refuses a configuration that spawns such children.

That is acceptable only while built-in tools are off. Before any harness may run
a shell, its instance must be held by a cgroup or a subreaper helper, so that
containment no longer depends on what a child chooses to reveal.

## Hive tools

`tinyhivemind-mcp` serves the hive vocabulary and the package's executable tools
over Streamable HTTP on a random port of 127.0.0.1.

- **Identity.** Each seat has its own random bearer, held in memory and in the
  instance's private config. The bearer alone decides the actor.
- **Turn.** Every call that changes state carries a `turn` argument that must
  equal the seat's running `turn_id`. A call with no running turn, or naming
  another turn, is refused. The bearer cannot do this alone: OpenClaw resumes
  an interrupted turn after a crash, and its calls would carry a valid bearer
  while a newer turn is running.
- **Call.** Every call that changes state also carries a required `call` string,
  chosen by the model and unique per intended action within the turn. See
  [call identity](#call-identity). `turn` and `call` are added to the served
  schema only and removed before hooks and dispatch; `check` rejects an executable
  tool whose own `args` declare either name.
- **Surface.** Only tools. Resource methods return an empty list, since Codex
  always offers resource readers. A bad `Host` or any `Origin` is refused, no
  redirect is ever sent, and request bodies are bounded.
- **Policy.** Package hooks, permission layers and send policy run here, before
  dispatch, for every tool this server serves.

`turn` protects against stale calls, not against a hostile model: the model can
read it. A seat acting within its own current turn has its own authority anyway.

### Call identity

The identity of a call is (seat, turn, `call`): the bearer gives the seat. A
bearer and a `turn` alone cannot tell a retry from a second intended action, and
no harness supplies a stable invocation id.

| Call | Outcome |
| --- | --- |
| new identity | acts |
| known identity, same tool and arguments | returns the recorded result; runs neither the tool nor its hooks |
| known identity, different tool or arguments | refused |
| another `call`, even with identical arguments | acts |
| `turn` not running, whatever the `call` | refused |

State-changing means everything but the list and read hive tools and executable
tools with `side_effects = "read"`. A proxied MCP call always counts as changing
state, and so does **any** call a package hook matches, reads included: a hook is
an executable and may act. A call refused by validation or policy, before any hook
runs, records nothing. When a hook matches, "started" is recorded before the first
hook, so a retry reruns no hook: it gets the recorded result or refusal, or is
refused while the outcome is unknown.

- **Coordinator actions** with no matching hook (post, ask, broadcast, complete, sends, create, join and
  leave hive): the record and its result are written in the same transaction as
  the effect, so neither exists without the other and a restart of the server
  alone does not lose it. A coordinator restart interrupts every running turn, so
  a later retry is refused by its `turn` rather than answered from the record.
  The record may therefore be dropped when its turn ends. A hooked coordinator
  action has both records: this one for the action, the next for its hooks.
- **Executable tools, proxied MCP calls, `create_agent` and any hooked call** act outside the
  coordinator's transaction. The server records "started" before it runs one, and
  the result after. A retry gets the recorded result or, while the outcome is
  unknown, a refusal. This is at most once **within one server process**; the
  records are in memory.
- **Not covered.** The model picks `call`, so a model that repeats an action under
  a new `call` acts twice. The rule makes transport retries safe. It does not
  make a model careful.

**Today the coordinator cannot record this.** `submit_action` pushes an action onto
the running turn with no identity, so a retried post is staged twice. Only sends
deduplicate, by `message_id`. The single-transaction commit that the record needs
does exist. [Plan step 2](../plans/2026-11-25-process-harnesses.md) adds it.

Two existing ids are not this one:

- **`message_id`** on a send stays the send's durable identity, across turns and
  restarts, and is one space for the whole coordinator: the same id with a
  different sender or payload is a conflict. It is checked after `call`. So two
  sends with different `call` values but the same `message_id` and payload are one
  message; a second intended message needs its own `message_id` too.
- **`THM_CALL_ID`** is the host's counter of *runs* of an executable tool or hook
  (`<id>-<n>`, `hook-<id>-<n>`), restarts with the process and is unknown to the
  caller. It is unchanged and takes no part in deduplication. A retry answered
  from the record runs nothing and so gets no new one.

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

What implementing it on 0.64.15 settled:

- No RPC reports the tools a custom agent's turn offers
  (`agent_list_definitions` and `agent_get_definition` cover built-in agents;
  the offline `openhuman agent dump-prompt --with-tools` CLI reports a
  definition's tools but not a session's). So the model-visible list is
  computed from the effective config the way `resolved_scope` in
  `agent/tinyagents/host/definition_registry.rs` builds it: the chat agent's
  allowlist, of the tools the instance has; every deferred tool if the list
  names `tool_search`; the session's delegation tools for an agent with
  subagents; `juice_retrieve` while compaction or the orchestrator's summary
  stage is on; the REPL tools while results are stored behind a handle; and
  `extra_tools`. A chat agent other than the written one counts as a
  built-in's set. Any of these fails the start. The config turns compaction
  off, since it defaults on. The live gate test checks the list at the model.
- A turn that fails before its thread has a transcript reports no session,
  as an error, so the next turn starts the thread rather than failing to
  resume one that never existed. A failure on a thread with a transcript is a
  failed turn in that session.
- The hashed names are known only to a running instance, so a start is two:
  one whose allowlist matches nothing, to read them, and the verified one.
- A session is a `thread_id`, the same for a seat's life. A turn that names a
  session runs only if `threads_transcript_get` has a transcript for it;
  otherwise the turn fails as a resume failure and nothing reaches the model.
- Every turn has its own `client_id`, and its `/events` subscription is open
  before `channel_web_chat` is called, since the first events can precede its
  answer. Events are taken only for the returned `request_id`, except
  `approval_request`, whose `request_id` is the approval's.
- A cancelled or failed turn leaves no message in the thread.
- An approval the harness asks for goes to the seat's approver, which has a
  time limit and is a denial past it. `approval_decide` takes no reason, so a
  denial's reason is not passed on. Approvals still pending when a turn starts
  are left by one that is over, and are denied first. A `chat_error` message is
  cut to one line of at most 200 characters before it becomes the failure.
- The top-level `embeddings_provider` is rewritten to the hosted default on
  load whatever the file says, so it is neither written nor compared; memory
  is off, so nothing embeds. The binary also carries a compiled-in Sentry DSN
  that no config or environment entry empties, which is one more reason live
  runs block egress.

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

A lock file records version, target and sha256 for each kind: `harnesses.lock`
at the repository root. A script, `.github/scripts/fetch-harness.sh`, fetches
and verifies; the library takes a path. Live tests run against exactly the pin,
with outbound network blocked except to the model endpoint under test.

## Acceptance

One conformance suite runs over the port, against a scripted fake process and
each kind: hive tool with the right actor; wrong bearer; another seat's bearer;
stale `turn`; second turn continues; resume after restart; resume failure;
timeout; drop stops the model call; kill mid-turn neither replays nor loses the
seat; per-turn usage; exact model-visible tool list; no process left after stop.

Call identity: a retry of one call acts once and returns the first result; two
identical calls under different `call` values in one turn both act; a reused
`call` with different arguments, or another tool, is refused; a missing `call` is
refused; a retried read with a hook reruns neither; an executable tool retried while its outcome is unknown is refused and
runs once.

Runtime gates: `read_only` refuses an effectful tool; `supervised` without an
approver refuses it; a `sandbox` other than `none` fails `check`.

Providers: each accepted row reaches exactly the declared endpoint; each rejected
row fails `check` at its location; with the declared endpoint down the turn fails
and no other endpoint is contacted; a credential appears in no log, argument list
or tool environment.

Live tests (`live_*`, opt-in) run the same cases on the pinned binaries, then a
mixed hive reaches a decision with every message in the coordinator transcript.
