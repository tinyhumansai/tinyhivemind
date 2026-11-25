# Conformance suite

The acceptance cases of the [harness spec](../../../../../docs/specs/harnesses.md)
that need no MCP server, over the port alone. Behind the `conformance` feature,
which a client crate enables as a dev-dependency.

| File | Purpose |
| --- | --- |
| `mod.rs` | `Case`, `Failure` and how to run them. |
| `subject.rs` | `Subject`: what a client provides to be tested. |
| `cases.rs` | The cases. |

It is a feature because other crates call it, so it cannot be a test module, and
a deployed host has no use for it, so it is not always compiled.

Not here, and not stubbed: hive tool with the right actor, wrong bearer, another
seat's bearer, and stale `turn`. They need the MCP server of step 2 of the
[plan](../../../../../docs/plans/2026-11-25-process-harnesses.md) and join
`Case` with it.
