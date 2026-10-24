# Portable hive conversions

Hive packages are read here and turned into what `deploy` builds from.

| File | Responsibility |
| --- | --- |
| `mod.rs` | Conversions of coordinator settings, lowered seat requests and the harness namespace portion of memory bindings |
| `package.rs` | `PackageSource`: `load` (read a directory and check it), `from_documents`, `new` (check a package a host edited), `read` (an unchecked `Package` to edit), and `root`, `package`, `lowered` accessors |
| `plan.rs` | Lowers one or more `PackageSource`s into the crate-internal deployment plan and the per-seat extras (executable tools, hooks, features, skills) |
| `plan_test.rs` | Shared seats must agree across packages; permissions and context may differ per hive |
| `test.rs` | Field preservation and refusals for layouts the harness cannot express |

`PackageSource::load` skips dot-files, and fails with `ConfigError::Package`
carrying every located finding when the package does not check. `plan` fails
with `ConflictingSeat` when a seat id two packages declare differs in its
integration, and `ConflictingDeclaration` when a provider, model, MCP server,
skill, tool, permission set or the runtime is declared differently under one
id. A seat's `permissions` and `context` may differ and become that hive's
narrowing overlay.

`management_request` passes every lowered seat setting through a portable
factory envelope. The original template configuration is nested under `config`,
alongside the resolved model name, tool access, and the ids of the seat's MCP
servers, skills and executable tools.
A memory request carries the identity, lifecycle, read-only/fork requirements,
recall moments, read reaches, character budget and remembered entry kinds.

Factories opt in through `AgentFactory::create_with_memory`, implement the whole
contract, apply `memory_binding` to an `AgentSpec`, then build the agent. The
harness binding represents only an agent id and root; it cannot itself enforce
read-only identities, bounded reaches, forks or recall policy. A host unable to
implement any requested setting must refuse creation. The default method
refuses explicit bindings before constructing an agent. The host verifies the
installed id and root before registration. Pool namespaces require a host
implementation; `memory_binding` refuses them because the harness stores turns
under `<root>/agent:<id>`.
