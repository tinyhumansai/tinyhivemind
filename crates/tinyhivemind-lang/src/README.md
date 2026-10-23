# Language source

All operations consume caller-owned values. No module opens storage or starts
an agent. `lib.rs` carries the crate overview and runnable example, and
re-exports the module surface: `check`, `check_packages`, `Review`,
`diagnose`, `diagnose_documents`, `Diagnostic`, `Rule`, `parse`, `validate`,
`validate_path`, `validate_namespace`, `lower`, and every wire type.

Editors change a hive by editing its files; `check` is the compiler that
reports every violation of the language and the constitution as a located
`Diagnostic`, or returns the edit's per-class patches for lineage. `patch` and `lineage`
keep their own `Error` and `Result`, so they are used through their module
paths.

| Module | Purpose |
| --- | --- |
| `check` | `check` and `check_packages`: direct edits checked against the original and the constitution, every finding collected; `Review` on success. |
| `diagnostic` | `Diagnostic` and `Rule`, and the collector every rule reports into. |
| `document` | `Package` and `Manifest` wire types, the integration sections (`document/integration/`), `parse` and `diagnose_documents` over a caller-supplied document map. |
| `memory` | `Constitution` and `MemorySpec`: caps, protected namespaces, identities, lifecycles, reaches and recall settings. |
| `validation` | `validate`, `diagnose`, `validate_path`, `validate_namespace`: references, paths, frontmatter agreement, goal acyclicity, policy bounds, memory access and the integration sections. |
| `lowering` | `lower`: core desks, routing candidates, host seat creation inputs and each seat's resolved integration. |
| `error` | The package-level `Error` returned by parsing, validation and lowering. |
| `patch` | The recorded form of an edit: `apply`, `guard`, and `diff`; learning operations; eight edit classes including `integration`. |
| `lineage` | Content identities, evaluated records, `accepted_head` and `pareto_frontier`. |
