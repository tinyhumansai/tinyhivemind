# `hook`

A seat's hooks: executable tools bound to an event.

| File | Holds |
| --- | --- |
| `mod.rs` | `HookRunner` (`pre_tool_use`, `post_tool_use`, `stop`, `is_empty`) and `Verdict` |
| `matcher.rs` | `*` wildcard matching of a hook's matcher against a tool name |
| [`test/`](test/README.md) | Verdicts, matchers, order, failing hooks and event objects |

Hooks of an event run in declaration order. `pre_tool_use` stops at the
first `{"permission": "deny"}`, whose `message` (or `refused by hook <id>`)
is the refusal; `HookRunner::stop` returns the first `{"followup": …}`, which the OpenHuman adapter observes and discards: a turn is authorized by the coordinator's round, so a stop hook cannot start one (see the spec's [hooks section](../../../../docs/specs/executable-tools.md#hooks)). Stdout that is
not JSON is no verdict. A failing hook is ignored, except that a
`fail_closed` `pre_tool_use` hook refuses the call with
`hook <id> failed: <error>`. A hook's `timeout_ms` replaces its tool's, and
each hook call gets `THM_CALL_ID` `hook-<id>-<n>`.
