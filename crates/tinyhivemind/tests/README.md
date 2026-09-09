# `tests`

Integration tests for `tinyhivemind`: the regression suite for its public API,
exercised only through `crate::` (well, `tinyhivemind::`) items a host actually
sees, never through a private path a module-local test could reach.

## Files

- `utterance_surface.rs` — a fixed script of tool calls played through
  `speech::interpret` and `speech::commit_utterance`, asserting the exact rows,
  audiences, routing and closing decision the room makes of them. It pins the
  run-28 path so that moving the meaning of an utterance out of a host cannot
  quietly change it.
- `joining_a_folded_room.rs` — what a seat taking its first turn on a long
  room is handed: the account, and only the rows it does not already cover.
  The guarantee the size trigger exists to make true by construction.
- `public_api.rs` — one test file, `root_*` and a handful of behavior tests,
  asserting that everything a host needs is re-exported from the crate root (or
  from the small number of documented submodules) and that it behaves the way
  its own module's unit tests already prove, when driven from outside the
  crate.

## What it covers

Each test targets one export surface rather than one code path — the module
that owns the behavior already has its own `test.rs` with the exhaustive
cases:

- `root_exports_runtime_records_and_constants` — `Conversation`, `SessionMessage`,
  `Elision`, and the paging constants (`SESSION_WINDOW`, `PAGE_SIZE`,
  `SCAN_LIMIT`) are reachable and hold their documented values.
- `root_exports_continuous_sharing_state` — `initialized_state` and
  `note_present` are reachable, and `PRESENT_SET_LIMIT` holds its documented
  value.
- `root_reexports_the_core_algebra` — a `tinyhivemind_core` item
  (`chat::same_conversation`) and a runtime-only responder type
  (`ResponderRung`, `SelectionDisposition`) are both reachable from the same
  crate.
- `root_exports_dispatch_outcomes_and_conversation_mapping` — `DispatchConversation`'s
  `From<&Conversation>` folds `"General"` to `chat::GENERAL_DESK`, and the
  dispatch/enqueue outcome enums are exported.
- `root_exports_search_records_and_constants` plus
  `the_search_viewer_argument_narrows_what_is_matched` — `SearchQuery`'s
  builder methods, the search constants, and — end to end, through a fake
  `SessionLog` — that a `Viewer` argument actually narrows which asides a
  search can match, not just that the type is exported.
- `root_exports_the_pin_fold_and_its_briefing_note` — `fold_pins`,
  `read_directives`, and `pin_note` are reachable and, as with search, that an
  outsider's board excludes a directive carried in an aside it cannot read.
- `root_exports_the_brevity_policy_stated_in_a_briefing` — `BrevityPolicy::DEFAULT`
  is reachable and its rendered rule text agrees with its own window.
- `the_referral_queue_port_is_available_to_consumers` — a from-scratch
  `ReferralQueue` implementation, built only from exported types, can drive
  `dispatch_referral` through a crossing referral, a duplicate trigger, and the
  conservative default that never reaches the queue.

## Constraints worth knowing

- **No private imports.** Every `use tinyhivemind::...` in this file must
  resolve through the crate's public surface; if a test needs something that
  is not exported, the fix is to export it (deliberately, from `src/lib.rs`)
  or move the test into the owning module's `test.rs`, never to reach past the
  boundary.
- **This is a surface test, not a logic test.** These tests do not attempt to
  re-cover a module's edge cases — that duplication belongs nowhere, since it
  would drift from the real tests instead of catching a real regression. They
  exist to catch an export accidentally dropped, renamed, or left private.
- **`#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`** is
  scoped to this file, matching every other test module in the crate: fine in
  a test, forbidden in library code.

## Where these run

`cargo test --all-features` runs this file alongside every crate's own unit
suite. There is no feature-gated or `live_*` test here — everything in this
file is deterministic and needs no network or host.
