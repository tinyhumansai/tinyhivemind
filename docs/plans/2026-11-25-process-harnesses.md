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

[Call identity](../specs/harnesses.md#call-identity) is not yet possible and needs
a change in `tinyhivemind-hives` first. Today `Coordinator::submit_action` appends
to `RunningTurn::actions` with no identity, `send` knows only the global
`message_id`, and create, join and leave hive know neither. `update` already
commits a whole `StoredState` in one compare-and-swap, so the change is:

- a serde-defaulted call record on `RunningTurn` (`call` to tool, arguments and
  result), so older snapshots load and it goes when the turn ends;
- identity-carrying variants of `submit_action`, `send` and the hive management
  calls that check `turn_id`, look up, act and record inside one `update`;
- a typed error for reuse and one for a stale turn. Existing entry points keep
  their behaviour for the linked adapter.

In the server: the started and finished record for executable tools, proxied MCP
calls, `create_agent` and any call a hook matches, and the `autonomy` gate.

Done when tests refuse: a wrong bearer, another seat's episode, a stale `turn`, a
call with no running turn, a forged actor, a bad `Host`, any `Origin`, an
oversized body; and when a package hook denies a call before it runs.

Also when tests show: a retried post, send, join and executable tool each act
once and return the first result; two identical posts under different `call`
values both act; reuse with different arguments or another tool is refused, as
are a missing `call` and a retry whose outcome is unknown; a failed commit leaves
neither effect nor record; the record survives a storage reload of a running turn
and an older snapshot still loads; after a coordinator restart the retry is
refused; a hook-denied call is recorded, a retry reruns no hook, and a corrected call uses
a new `call`; two sends with
one `message_id` under different calls are one message.

## 3. Schema

`tinyhivemind-lang`: the `harnesses` section, `harness` on runtime, profile and
seat, lowering, the capability table including the provider rows,
`Rule::Integration` diagnostics, and the two-class patch rule. Touch `document/{types,defaults}.rs`,
`document/integration/types.rs`, `document/split`, `validation/integration.rs`,
`lowering/{types,integration}.rs`, `patch/mod.rs`, READMEs and example packages.

Until step 5 the OpenHuman deployment refuses any seat whose harness is not the
default, so the field is never accepted and ignored.

Done when: existing package ids are unchanged; unknown, duplicate and unsupported
cases are diagnosed at their location; a roster-only edit cannot change `harness`.

## 4. Clients

- **4a. `tinyhivemind-openclaw`:** handshake with Ed25519 device identity,
  `agent` and its second response, `chat.abort`, generated config, recovery hold.
- **4b. OpenHuman process runner** in `tinyhivemind-openhuman-process`, a crate
  beside the linked adapter rather than a module of it, so that step 6 deletes
  the adapter without touching it: workspace and config per seat, `web_chat` and
  `/events`, `web_cancel`, start-up verification, approvals through the
  `approval_request` event and `decide`. Done: conformance offline and `live_*`
  on 0.64.15.
- **4c. `tinyhivemind-codex`:** app-server over stdio, pinned catalog, per-thread
  MCP. Medulla's `codex_app_server` is the reference for framing; its silent new
  thread on a failed resume is not copied.

Each is done when it passes conformance offline and `live_*` on its pin. The first
conformance test added is a model endpoint that requires the right key: until it
passes, `credential` stays rejected and no hosted model is usable. `openrouter`,
`anthropic` and further endpoint shapes follow the same way, one row each.

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
