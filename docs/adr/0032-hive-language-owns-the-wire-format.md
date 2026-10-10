# 32. The hive language owns a host-neutral wire format

- **Status:** Accepted
- **Date:** 2026-10-10
- **Supersedes:** [ADR 0028](0028-three-crate-workspace.md) package count

## Decision

Add `tinyhivemind-lang` as the fifth workspace crate. It owns the declarative
hive package, typed editing and lineage format. It depends on core and pure
serialization/hash libraries; it never depends on the coordinator or harness.
`assert-pure.sh` enforces that boundary. The existing four crates retain their
responsibilities. Language memory bindings are host-neutral; the OpenHuman
adapter converts them to harness bindings and coordinator options.

## Consequences

A wire-format boundary justifies a package even though mechanics stay in core.
Hosts can parse, validate, lower and evaluate candidates without a runtime.
The host chooses when to activate a version and stores memory and lineage.
Persistent memory is explicit, opt-in, and pins host watermarks in lineage;
the library cannot promise snapshot reads or copy-on-write for a host store.
The constitution is human/host-owned and cannot be changed by editor patches.
