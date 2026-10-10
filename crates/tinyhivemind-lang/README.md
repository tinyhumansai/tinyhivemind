# tinyhivemind-lang

Portable hive definitions and guarded edits for a host that owns agents,
transcripts, evaluation and memory. The crate accepts an in-memory document
map and performs no file, socket or store access. See the
[repository overview](../../README.md) and
[language specification](../../docs/specs/hive-language.md).

Call `parse` for canonical JSON plus Markdown side documents, `validate` before
activation, and `lower` to obtain existing core desks and routing candidates
with declared roles. Lowered seat configuration preserves memory constraints;
the host must enforce those when registering a runner.

The immutable constitution pins evaluator settings and hard caps. The host
must use guarded patch application before activating an editor proposal.
Accepted and rejected evaluated candidates remain host-owned lineage records.

| Directory | Purpose |
| --- | --- |
| src | Typed wire definitions, validation, lowering, edits and lineage. |
| tests | Checked-in package fixtures and public API integration evidence. |
| examples | Deterministic host-driven self-edit demonstration. |
