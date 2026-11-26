# Native hive tools

`mod.rs` wraps each of the host's hive tools
([README](../../../tinyhivemind-host/src/tools/README.md)) as a native
`tinytools::Tool`. The wrapper binds attribution to the agent ID, reaches the
host through a weak reference, refuses calls before registration completes, and
renders the result or the error as the tool's text. Vocabulary, argument
validation, policies and dispatch are the host's.

The order of refusals is fixed: arguments, then an unavailable host, then pending
registration, then the send policy, then the operation.

| File | Responsibility |
| --- | --- |
| `mod.rs` | The wrapper and the per-seat belt |
| `test.rs` | Definitions, impersonation, messaging, authorization and membership through native calls |
| `direct_test.rs` | The native send/read path |
| `policy_test.rs` | The adapter's `SendAuthorizer` through native calls |
