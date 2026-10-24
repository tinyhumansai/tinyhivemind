# Package equivalence tests

`manifests.rs` reads and checks each checked-in package through the adapter's
public `PackageSource`, pins that every canonical `hive.json`,
`constitution.json` and `memory.json` is its own complete serialization, and
then pins the seat identities, roles, tool scopes, MCP servers and allowlists,
iteration and sampling limits, workspace context and cross-hive narrowing that
the binaries depend on. Its last test deploys every package set on the offline
native runtime and compares agent ids, memory roots, limits, roles and episode
policy with the lowered packages. The executable offline proofs also run actual
provider requests and native tools.
