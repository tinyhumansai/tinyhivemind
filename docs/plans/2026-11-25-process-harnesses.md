# Process harnesses

Implements [harnesses](../specs/harnesses.md) under
[ADR 0035](../adr/0035-a-harness-is-a-process.md) and
[ADR 0036](../adr/0036-hive-tools-are-served-over-loopback-mcp.md).
Every step before 6 leaves the linked OpenHuman path working.

Order: 1, then 2 and 3 in parallel, then 4a, 4b and 4c in parallel, then 5, 6, 7.

## 1. Extract `tinyhivemind-host`

Move, with no behaviour change, from `tinyhivemind-openhuman`: the tool
vocabulary (`tools/types.rs`) and dispatch (`tools/mod.rs`), send and management
authorizers, `TurnScope`, `TurnOptions`, `host/activation.rs`, `journal/`,
prompt rendering, `language/{package,plan}.rs` and the neutral part of `config/`.
Add the `Harness` port, neutral usage, progress and approval types, and process
supervision (start from `tinyhivemind-exec/src/run/process.rs`).

Done when: the adapter's tests pass unchanged; a scripted fake process passes the
conformance suite; a test shows a grandchild in its own session is also stopped.

## 2. `tinyhivemind-mcp`

Streamable HTTP server over the dispatch of step 1, with per-seat bearers and the
`turn` check. Choose the HTTP dependency in this step and record why.

Done when tests refuse: a wrong bearer, another seat's episode, a stale `turn`, a
call with no running turn, a forged actor, a bad `Host`, any `Origin`, an
oversized body; and when a package hook denies a call before it runs.

## 3. Schema

`tinyhivemind-lang`: the `harnesses` section, `harness` on runtime, profile and
seat, lowering, the capability table, `Rule::Integration` diagnostics, and the
two-class patch rule. Touch `document/{types,defaults}.rs`,
`document/integration/types.rs`, `document/split`, `validation/integration.rs`,
`lowering/{types,integration}.rs`, `patch/mod.rs`, READMEs and example packages.

Until step 5 the OpenHuman deployment refuses any seat whose harness is not the
default, so the field is never accepted and ignored.

Done when: existing package ids are unchanged; unknown, duplicate and unsupported
cases are diagnosed at their location; a roster-only edit cannot change `harness`.

## 4. Clients

- **4a. `tinyhivemind-openclaw`:** handshake with Ed25519 device identity,
  `agent` and its second response, `chat.abort`, generated config, recovery hold.
- **4b. OpenHuman process runner** in `tinyhivemind-openhuman`, beside the linked
  path: workspace and config per seat, `web_chat` and `/events`, `web_cancel`,
  start-up verification, approvals through `list_pending` and `decide`.
- **4c. `tinyhivemind-codex`:** app-server over stdio, pinned catalog, per-thread
  MCP. Medulla's `codex_app_server` is the reference for framing; its silent new
  thread on a failed resume is not copied.

Each is done when it passes conformance offline and `live_*` on its pin.

## 5. Mixed deployment

One coordinator, a harness per seat, agreement across packages, an example hive
with all three kinds, and coordinator-run workflows.

## 6. Remove the link

Gate: every field of the integration spec is working or rejected per kind, no
declared permission is unenforced, consumers (`rsi-ralph`, and OpenCompany once
confirmed) have a path, and the working memory question in #104 is settled.
Then delete the linked modules, `vendor/openhuman`, the patch tables and
`assert-openhuman-pin.sh`, and extend `assert-pure.sh` to bar every harness.

Upstream asks this depends on: released `tool_rules` with unknown fields
rejected; hook deny as a tool result with agent and turn ids; a configurable
iteration cap with `hit_cap` in `chat_done`. Useful, not blocking: `agent_id` on
`web_chat`, caller-supplied turn id, failure on bad config and a busy port.

## 7. Migrate `rsi-ralph`

A separate change in that repository, after step 6, or never: it may stay on the
last linked commit.
