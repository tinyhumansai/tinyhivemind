# Plan: Desks

- **Status:** Implemented
- **Specification:** [`../specs/desks.md`](../specs/desks.md)

## Goal

Add the pure, borrowed desk-overlay algebra to `tinyhivemind-core`, including its
stable host wire types and typed validation errors, without changing P1 chat
identity behavior or introducing I/O, async, or host types.

## Task 1: Pin DTO wire representations

**Files:** `crates/tinyhivemind-core/src/desk/types.rs`,
`crates/tinyhivemind-core/src/desk/test.rs`, `crates/tinyhivemind-core/Cargo.toml`,
`Cargo.toml`

1. Restore `serde` to the core manifest and add `serde_json` as a test-only
   workspace dependency for representation tests.
2. Add failing tests asserting the exact serialized keys and round trips for
   `Desk`, `DeskMember`, and `DeskOrder`.
3. Implement the three documented owned-string DTOs with serde derives.
4. Run `cargo test -p tinyhivemind-core desk::test::wire`.

## Task 2: Add typed validation errors

**Files:** `crates/tinyhivemind-core/src/error/mod.rs`,
`crates/tinyhivemind-core/src/error/test.rs`, `crates/tinyhivemind-core/src/lib.rs`,
`crates/tinyhivemind-core/Cargo.toml`

1. Add failing display tests for each error variant, including the requirement
   that messages start lowercase and have no trailing punctuation.
2. Restore `thiserror` and implement crate-wide `Error` and `Result<T>`.
3. Publish the error module without flattening it at the crate root.
4. Run `cargo test -p tinyhivemind-core error`.

## Task 3: Validate desks and resolve identities

**Files:** `crates/tinyhivemind-core/src/desk/mod.rs`,
`crates/tinyhivemind-core/src/desk/test.rs`

1. Add failing tests for empty id/name, exact duplicate ids, canonical General,
   every non-default reserved collision, exact id/name lookup, case sensitivity,
   unknown lookup, and ambiguous names.
2. Implement private iteration in declared-then-added order, `validate`,
   `resolve_id`, and `contains`.
3. Run `cargo test -p tinyhivemind-core desk::test`.

## Task 4: Merge membership overlays

**Files:** `crates/tinyhivemind-core/src/desk/mod.rs`,
`crates/tinyhivemind-core/src/desk/test.rs`

1. Add failing tests proving founding-before-added order, first-appearance
   deduplication, exact retirement, unknown member targets, and empty/non-empty
   lead selection.
2. Implement the borrowed merge and `members`/`lead` APIs.
3. Run `cargo test -p tinyhivemind-core desk::test`.

## Task 5: Require whole-set orders

**Files:** `crates/tinyhivemind-core/src/desk/mod.rs`,
`crates/tinyhivemind-core/src/desk/test.rs`

1. Add failing tests for an accepted permutation, unknown order target,
   duplicate order target, duplicate member, unknown member, and missing member.
2. Validate every order against the deduplicated, non-retired final set and
   return only complete accepted orders from `members`.
3. Run `cargo test -p tinyhivemind-core desk::test`.

## Task 6: Publish and document

**Files:** `crates/tinyhivemind-core/src/lib.rs`,
`crates/tinyhivemind-core/tests/public_api.rs`, `docs/specs/README.md`,
`docs/plans/README.md`, `docs/specs/desks.md`, `docs/plans/desks.md`,
`ROADMAP.md`

1. Publish `desk` beside `chat`, keep public names namespaced, and update crate
   docs/examples for the now-fallible desk API.
2. Exercise downstream construction and lookup in the integration suite.
3. Mark the spec and plan implemented and P2 done/P3 next only after all checks
   below pass.

## Verification

- [x] Focused red/green desk and error tests
- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy -p tinyhivemind-core --all-targets --all-features -- -D warnings`
- [x] `cargo test -p tinyhivemind-core --all-features`
- [x] `.github/scripts/assert-pure.sh`

## Completion

P2 is implemented. The public API, docs, wire tests, typed error paths,
membership merge, and stable whole-set ordering pass the verification above;
P3 may build mention resolution against this desk surface.
