# Portable hive conversions

`PackageSource`, the package-to-plan lowering, `coordinator_options` and
`management_request` are `tinyhivemind-host`'s
([README](../../../tinyhivemind-host/src/language/README.md)), re-exported here
under their established paths. This module adds the harness part.

| File | Responsibility |
| --- | --- |
| `mod.rs` | The re-exports, and the harness namespace portion of memory bindings |
| `test.rs` | Refusals for memory layouts the harness cannot express |

Factories opt in through `AgentFactory::create_with_memory`, implement the whole
contract, apply `memory_binding` to an `AgentSpec`, then build the agent. The
harness binding represents only an agent id and root; it cannot itself enforce
read-only identities, bounded reaches, forks or recall policy. A host unable to
implement any requested setting must refuse creation. The default method
refuses explicit bindings before constructing an agent. The host verifies the
installed id and root before registration. Pool namespaces require a host
implementation; `memory_binding` refuses them because the harness stores turns
under `<root>/agent:<id>`.
