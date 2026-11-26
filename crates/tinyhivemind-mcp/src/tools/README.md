# Tools

The catalog of one seat, and the path of one `tools/call`.

A tool changes state unless it is a list or read hive tool or a program
declared `side_effects = "read"`, and any tool a `pre_tool_use` or
`post_tool_use` hook matches changes state too, reads included. Such a tool's
schema gets the required `turn` and `call`, which dispatch removes before
anything else sees the arguments.

A call then goes: turn check (refused unless it is the seat's running turn),
argument validation (a hive tool's declaration, or a program's `args` schema),
and one of two records.

- A coordinator action with no matching hook goes to the host's belt as an
  identified call; the coordinator records it in the transaction of its effect.
- Everything else (executable tools, `create_agent`, any hooked call) uses the
  in-memory `Ledger`: a known call is answered from it, or refused while its
  outcome is unknown or when its `call` was used for something else; then the
  host's send and management policy (`Belt::authorize`) for a hive tool; then
  "started" is recorded, only if the coordinator still says the turn is
  running; then the access gate for a program (a denial drops the record);
  then the turn is checked again, the hooks and tool run, and
  "finished" is recorded. A hooked coordinator action has both records.

A refusal before any hook runs records nothing. The belt checks send and
management policy again at dispatch, so a policy that changes in between is
judged by that second check. Each call runs on its own task,
so a client that hangs up does not cancel it halfway.

| File | Responsibility |
| --- | --- |
| `mod.rs` | `Catalog`: what is served, under which schema |
| `dispatch.rs` | One call, from arguments to result |
| `ledger.rs` | Started and finished records of out-of-transaction calls |
| `gate.rs` | `read_only`, `supervised` with the host's approver, `autonomous` |
| `schema.rs` | Executable tool arguments checked against their declared `args` |
| `test.rs` | Ledger turn rules and bounds; the schema checker |
