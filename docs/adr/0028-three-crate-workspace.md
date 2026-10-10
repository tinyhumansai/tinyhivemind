# 28. Consolidate the workspace into three crates

- **Status:** Accepted; package count superseded by [ADR 0032](0032-hive-language-owns-the-wire-format.md)
- **Date:** 2026-10-03
- **Supersedes:** The package boundaries in [ADR 0025](0025-the-driver-names-no-harness.md)

## Context

The workspace split host-neutral work across seven packages. The hive folds,
session ports, routing types, TypeSafe router, and completion driver shared
several types and were routinely used together. Their Cargo boundaries added
imports and re-exports without independent deployment or storage ownership.
The OpenHuman adapter and episode tools have real dependency boundaries.

## Decision

Keep three packages. `tinyhivemind-core` owns all host-neutral logic in focused
modules: the original algebra, `runtime`, `hive`, `embed`, `typesafe`, and
`driver`. It opens no storage or transport, does not name OpenHuman types, and
retains host-supplied waiting ports. The pure fold and bounded-round invariants
are unchanged.

`tinyhivemind-tools` depends on core and `tinytools`. It renders served episode
vocabulary as native `tinytools::ToolSpec` values and keeps the call record.
`tinyhivemind-openhuman` depends on both and remains the only crate that links
the harness. The old package names are removed; consumers update imports.

## Consequences

The dependency graph is core <- tools <- OpenHuman, with OpenHuman also using
core directly. Core's strict dependency check remains in CI. Tools permits the
`anyhow` and `async-trait` dependencies carried by `tinytools`, while the check
still rejects transport and harness dependencies there. Existing serialized
payloads, routing decisions, and episode behavior remain stable. Historical
ADRs and experiments retain their original package names as dated records.
