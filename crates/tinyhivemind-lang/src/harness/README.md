# Harness kinds

What a seat runs on, per the [harness spec](../../../../docs/specs/harnesses.md):
the kinds, the `harnesses` section's entries, how a seat resolves to one, and
the capability table every later crate queries.

Public surface: `HarnessKind` (with `ALL`, `default_isolation`, `support`,
`model_api` and `iteration_cap`), `Harness` and `Isolation` (the wire
types), and `Capability`, `Support`, `Unlock` and `ModelApi`.

The table is data, queried by `(kind, capability)`: `check` asks it for every
setting a seat resolves to, a process runner asks it before writing a harness
config, and plan step 6's gate enumerates `Capability::ALL` against
`HarnessKind::ALL`. A rejected row carries its `Unlock`, so the rows only a
conformance test can move (`Unlock::Conformance`) are named in code. Moving a
row means editing `Capability::row` and the literal table pinned in `test.rs`
in the change that proves it.

`resolve.rs` is crate-private: a seat's own `harness`, else its profile's,
else `runtime.harness`, and where it came from. Lowering, validation and the
edit rules all resolve through it. `moves` finds the seats an edit moves to
another harness, which needs the `integration` class.

| File | Purpose |
| --- | --- |
| `mod.rs` | `HarnessKind` and the module surface |
| `types.rs` | `Harness` and `Isolation` |
| `capability.rs` | The capability table, model API and iteration cap per kind |
| `resolve.rs` | Resolution by precedence, and harness moves between two manifests |
| `test.rs` | Wire names and forms, absent-field omission, and the pinned table |
