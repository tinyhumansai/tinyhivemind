# Plan: Chat identity

- **Status:** Implemented
- **Specification:**
  [`../specs/chat-identity.md`](../specs/chat-identity.md)

## Goal

Relocate the General-desk equivalence logic from `opencompany`'s
`src/server/chat_history.rs` into `tinyteams-core::chat`, byte-for-byte
identical in behavior, with no new dependency and no crate-wide `Error` type
(nothing here is fallible).

## Task 1: Add the module skeleton and the General-desk predicate

**Files:** `crates/tinyteams-core/src/chat/mod.rs`,
`crates/tinyteams-core/src/chat/test.rs`

1. Add a failing test per accepted spelling:

   ```rust
   #[test]
   fn an_unaddressed_chat_is_general() {
       assert!(is_general_chat(None));
   }
   ```

   repeated for `Some("")`, `Some("main")`, and `Some("General")`, plus one
   negative case for an unrelated desk id.
2. Implement `MAIN_THREAD_ID`, `GENERAL_DESK`, and `is_general_chat` exactly as
   specified, matching `opencompany`'s existing fold.
3. Run `cargo test -p tinyteams-core chat`.

## Task 2: Add conversation equivalence

**Files:** `crates/tinyteams-core/src/chat/mod.rs`,
`crates/tinyteams-core/src/chat/test.rs`

1. Add failing tests for: two different General spellings, a General/
   non-General pair (both directions), a matching non-General pair, and a
   case-differing non-General pair.
2. Implement `same_conversation` in terms of `is_general_chat`.
3. Run `cargo test -p tinyteams-core chat`.

## Task 3: Publish and document

**Files:** `crates/tinyteams-core/src/lib.rs`,
`crates/tinyteams-core/examples/basic.rs`, `ROADMAP.md`

1. Declare `pub mod chat;` from `lib.rs` and document the module in the
   crate-level overview.
2. Add a runnable example exercising `is_general_chat` and
   `same_conversation`.
3. Mark P1 `done` in `ROADMAP.md`.

## Task 4: Remove scaffolding made obsolete by real code

**Files:** `crates/tinyteams-core/src/greeting/`,
`crates/tinyteams-core/src/error/`, `crates/tinyteams-core/Cargo.toml`

1. Delete the template's `greeting` module and its coverage-filler role now
   that `chat` provides real, fully covered code.
2. Delete the template's empty `error` module and the `thiserror` dependency;
   nothing in this crate is fallible yet. `Error`/`Result` return in P2 when
   `desk::validate` needs them.
3. Remove the `serde` dependency; conversation identity is `&str` in, `bool`
   out. `serde` returns in P2 with the desk overlay types.
4. Run the full verification list below and confirm
   `check-file-coverage.sh 90` still passes on every remaining source file.

## Task 5: Full verification

- [x] `cargo fmt --all -- --check`
- [x] `cargo clippy --all-targets --all-features -- -D warnings`
- [x] `cargo build --all-targets --all-features`
- [x] `cargo test --all-features`
- [x] `cargo test` (default features)
- [x] `cargo run -p tinyteams-core --example basic`
- [x] `.github/scripts/assert-pure.sh`
- [x] `.github/scripts/check-file-coverage.sh 90 coverage.json`
- [x] `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features`
- [x] `cargo deny check`

## Completion

P1 is implemented and merged into this repository. The paired `opencompany`
PR bumps `vendor/tinyteams` and switches its own `chat_history.rs` call sites
to `tinyteams_core::chat`, pinning `GENERAL_DESK` against
`language::DEFAULT_DESK` with a compile-time assertion rather than
deduplicating the two — they are different concerns (desk identity vs.
prosumer glossary vocabulary) that share a literal by coincidence.
