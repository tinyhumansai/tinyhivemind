# Evaluated designs

The host retains candidates and records. `record` and `verify` bind an evaluation
to a SHA-256 digest of canonical JSON and every hive memory identity's event
watermark. Run memory is scoped to execution; persistent memory is part of the
evaluated design. Objective directions are explicit and scores must be finite.

| File | Purpose |
| --- | --- |
| `mod.rs` | Content addressing, record verification, accepted head and Pareto fold |
| `types.rs` | Serializable evidence and typed archive failures |
| `test.rs` | Identity, wire and archive fold contracts |

Archives are ordered, unique designs with earlier parent references. Rejected
designs never become accepted heads but can remain on the Pareto frontier.
The host verifies each record against its stored candidate before archive use.
