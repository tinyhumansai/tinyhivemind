# Chat identity: the four spellings of the default desk

- **Status:** Implemented
- **Owner:** Maintainers
- **Plan:** [`../plans/chat-identity.md`](../plans/chat-identity.md)

## Problem

A message is journaled under a chat id, and every surface that reads the
journal back — history rendering, thread resumption, an agent's context seed
— has to answer the same question about it: is this the conversation I am
asking about? The default desk in particular is addressed under four
different stored spellings depending on which surface wrote the message. If
each caller answers "same conversation?" with its own ad-hoc string compare,
the default desk's transcript silently splits across whichever id happened to
write each message — for example a remembered thread root stored as `None`
failing to match the `"General"` id it is rendered under, so a threaded
continuation resumes in the channel instead of its thread.

This behavior already exists, byte-for-byte, in `opencompany`'s
`src/server/chat_history.rs`. This is a relocation into the `tinyhivemind-core`
crate that both `opencompany` and future `tinyhivemind` consumers can share, not
a redesign.

## Goals

- Give one authoritative answer to "does this stored chat id mean the General
  desk?" across all four accepted spellings.
- Give one authoritative answer to "do two stored chat ids name the same
  conversation?", built on the first.
- Preserve the host's existing ASCII-case-insensitive folding for the General
  desk's spellings exactly; do not change stored-data semantics.

## Non-goals

- Desk membership, rosters, or mentions — those land in later phases (P2, P3).
- Unicode case folding, or case-insensitivity for any desk other than General.
  Non-General desk ids are opaque identifiers; two desks differing only in
  case are two desks.
- A crate-wide `Error` type. Every function here is total — a fold over data
  the caller already holds — so there is nothing to fail. `Error` arrives in
  P2 when `desk::validate` needs one.
- Serialization. Conversation identity is `&str` in, `bool` out; `serde`
  returns in P2 with the desk overlay types whose attributes are a wire
  format the host stores.

## Proposed behavior

```rust
use tinyhivemind_core::chat::{GENERAL_DESK, MAIN_THREAD_ID, is_general_chat, same_conversation};

assert!(is_general_chat(None));
assert!(is_general_chat(Some("")));
assert!(is_general_chat(Some(MAIN_THREAD_ID)));
assert!(is_general_chat(Some(GENERAL_DESK)));
assert!(!is_general_chat(Some("engineering")));

assert!(same_conversation(None, Some("General")));
assert!(same_conversation(Some("main"), Some("")));
assert!(!same_conversation(Some("engineering"), Some("Engineering")));
```

`MAIN_THREAD_ID` (`"main"`) is the console's default/orchestrator thread id;
`GENERAL_DESK` (`"General"`) is the default desk's own id and display name.
Both constants and both functions are public under `tinyhivemind_core::chat`.

## Invariants and constraints

- The four accepted spellings of the General desk are exactly: `None`,
  `Some("")`, a case-insensitive match on `MAIN_THREAD_ID`, and a
  case-insensitive match on `GENERAL_DESK`. No other input is General.
- `same_conversation(a, b)` is true when both `a` and `b` are General
  (regardless of which spelling each uses), or when `a == b` verbatim for any
  non-General id. It is never true when exactly one side is General.
- Folding is `eq_ignore_ascii_case`, not full Unicode case folding, matching
  the host's existing semantics on stored data.
- Both functions are pure, take borrowed `&str` input, return `bool`, and
  perform no I/O.

## Acceptance criteria

- All four General spellings are asserted individually (not via a loop that
  could pass while silently testing one case four times).
- `same_conversation` is asserted symmetric across a General/non-General pair,
  across two different spellings of General, and across a case-sensitive
  non-General pair.
- Formatting, Clippy, build, tests (including doctests), rustdoc, and
  `cargo deny` pass; `check-file-coverage.sh 90` passes on every changed file.

## Open questions

None. This phase is a relocation of existing, already-shipped behavior.
