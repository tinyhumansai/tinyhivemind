# Hive tools

`types.rs` declares the nine base tools and four optional management tools as
`Kind`. The same field declarations render JSON schemas and validate arguments.
Unknown arguments, including a supplied sender, are rejected. Integer sequences
must be nonnegative; recipient lists contain strings.

`ToolCall` is a validated call: it cannot be built from arguments that do not
fit, and its actor and session come from the host, never from an argument.
`Belt` executes one. Reads and sends are admitted by coordinator membership and
visibility checks; episode actions require the caller's active episode.

`ToolCall::in_turn(turn_id, call)` identifies a call as one action of the
actor's running turn. The belt then sends, stages and manages hives through the
coordinator's `*_in_turn` entry points, which refuse a turn that is not running
and answer a retry from the record written with the effect. `create_agent` is
the creator's, outside that record. Unidentified calls behave as before.

`hivemind_read` requires exactly one of `hive_id` or `agent_id`. Hive reads may
include `thread`; direct reads return only the caller's conversation with that
peer. `after` is an exclusive sequence cursor in both. Reading schedules nothing.

With a send policy, the two sends, `hivemind_ask` and `hivemind_broadcast` offer
it a `SendRequest` first; a refusal enqueues and stages nothing. Management
consults its authorizer before any mutation, and reaches a harness only through
`policy::AgentCreator`. Without it, the management tools are refused.

| File | Responsibility |
| --- | --- |
| `mod.rs` | `Belt`: policies, then the coordinator |
| `call.rs` | `ToolCall` and what its arguments mean |
| `types.rs` | `Kind`: names, schemas and validation |
| `test/` | Behavior tests; see [`test/README.md`](test/README.md) |
