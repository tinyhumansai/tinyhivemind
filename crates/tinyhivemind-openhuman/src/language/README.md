# Portable hive conversions

`mod.rs` converts coordinator settings, lowered seat requests and the harness
namespace portion of memory bindings. `test.rs` pins their field preservation
and refusals for layouts the harness cannot express.

`management_request` passes every lowered seat setting through a portable
factory envelope. The original template configuration is nested under `config`.
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
