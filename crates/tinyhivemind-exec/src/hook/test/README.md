# `hook` tests

| File | Covers |
| --- | --- |
| `mod.rs` | Wires the submodules |
| `support.rs` | Hooks over the fixture package, a runner, and the recorded event objects |
| `matcher.rs` | `*` wildcards |
| `pre_tool_use.rs` | Refusals, matchers, order, `fail_closed`, and the hook timeout |
| `events.rs` | `post_tool_use` and `stop` event objects, follow-ups, and `is_empty` |
| `fixtures/tools/` | POSIX `sh` scripts the tests run |
