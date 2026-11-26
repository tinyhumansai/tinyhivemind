# Capability

`check` holds a `LoweredSeat` to the `openhuman` column of the capability
table in [the spec](../../../../docs/specs/harnesses.md#capabilities) and
returns the `SeatModel` the config is written from, or the first `Refusal`.

- Providers: only `openai` with an explicit endpoint and no credential. No
  model, no provider (inherit), `inherit`, `openrouter`, `anthropic`, a missing
  endpoint and a credential are each a `Refusal`.
- `temperature` and `max_tokens` are refused: nothing proves the binary honours them.
- `limits.iterations` above `ITERATION_CAP` (8) is refused; below it, a turn
  past the limit fails. `limits.budget_usd` is refused.
- A sandbox other than `none`, a built-in tool group not `off`, `memory`,
  `agent_id`, `definition_base`, `subagents`, `skills`, `context` and
  `bare_prompt` are refused.
- Hive and executable tools, hooks, permissions other than `sandbox`,
  `autonomy`, `access` and package MCP servers are host side and not judged here.

| File | Purpose |
| --- | --- |
| `mod.rs` | `check`, `Refusal`, `SeatModel`, `ITERATION_CAP` |
| `test.rs` | Every row and refusal |
