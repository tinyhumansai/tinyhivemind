# Implement the hive language

Binding specification: [hive language](../specs/hive-language.md).
Issue: https://github.com/tinyhumansai/tinyhivemind/issues/112

## 1. Pure package and lowering

Create `crates/tinyhivemind-lang` with focused document, memory, validation,
lowering and error modules. Reuse core policies and add serde to DivisionPolicy
and ConductPolicy. Pin JSON shapes with tests before implementing validation.
Parse only caller-supplied documents; validate references, bounds, package
paths, memory syntax and constitution restrictions. Lower desks and candidates
with roles preserved, and return host-neutral runtime configuration.

## 2. Typed edits and lineage

Add patch, guard and lineage modules. Test stale/class/protected/invariant
failures before implementing atomic application. Itemized text operations
preserve unrelated entries. Test diff followed by apply returns the target.
Use a pure cryptographic hash for content IDs and Pareto folds with comparable
finite objective vectors. Pin persistent memory watermarks in records.

## 3. Adapter conversion

Add serde to CoordinatorOptions, preserving defaults. Add optional memory
binding to CreateAgent and install it through register_spec-compatible
registration. Conversion helpers accept language lowering values and return
existing harness bindings/coordinator options without coupling lang upward.
Test backward-compatible wire decoding and memory ID/root conversion.

## 4. Fixtures and demonstration

Add DeepSWE and PE1006 in-memory/package fixtures matching existing example
rosters and policies. Add a deterministic self-edit example demonstrating
propose, evaluation, accept/reject and lineage. Document all source folders.
Update workspace indexes, ADR index and purity assertions.

## 5. Integration checks

Run fmt, clippy all targets/features, build all targets/features, test all
features, purity assertion and rustdoc with warnings denied. Review the full
diff, address findings, then push the branch and open a ready upstream PR.
