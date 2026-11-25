# Hive packages

Hive packages are read here and turned into what an adapter deploys.

| File | Responsibility |
| --- | --- |
| `mod.rs` | Conversions of coordinator settings and lowered seat requests |
| `package.rs` | `PackageSource`: `load` (read a directory and check it), `from_documents`, `new` (check a package a host edited), `read` (an unchecked `Package` to edit), and `root`, `package`, `lowered` accessors |
| `plan/` | Lowers one or more `PackageSource`s into the deployment plan; see [`plan/README.md`](plan/README.md) |
| `package_test.rs` | Loading, editing and refusing packages |
| `test.rs` | Field preservation of the conversions |

`PackageSource::load` skips dot-files, and fails with `ConfigError::Package`
carrying every located finding when the package does not check.

`management_request` passes every lowered seat setting through a portable
factory envelope. The original template configuration is nested under `config`,
alongside the resolved model name, tool access, and the ids of the seat's MCP
servers, skills and executable tools. A memory request carries the identity,
lifecycle, read-only/fork requirements, recall moments, read reaches, character
budget and remembered entry kinds.
