# Implement attributed session projection and initialization

Implements [`../specs/sessions.md`](../specs/sessions.md).

## Goal and boundaries

Add the runtime crate, its host-owned log port, validated paging walk,
attributed transcript records, and ephemeral team briefing. Do not implement
watermarks, response selection, mention dispatch, storage, or a runtime choice.

## Test-first tasks

1. Create `crates/tinyhivemind/src/session/types.rs` and pin the serde forms of
   sequence, conversation, authors, raw rows, pages, projected messages, and
   queries in `session/test.rs` before implementing the records.
2. Add the object-safe `SessionLog` port and compile a trait-object test before
   implementing a fake log.
3. Add failing projection tests for zero windows, exclusive bounds,
   pagination, ordering, filtering, channel/thread behavior, blank content,
   attribution, the scan cap, and source failures; implement the minimal paging
   fold in `session/mod.rs`.
4. Add failing malformed-page tests, then implement typed validation for range,
   descending order, duplicates, empty-page cursors, strict cursor advancement,
   and the boundary that a next cursor is no newer than the oldest row.
5. Add failing briefing tests, then implement records, snapshot construction,
   deterministic `system_text`, and `initialize_session` under `briefing/`.
6. Add crate docs, module READMEs, centralized exports, manifest dependencies,
   and public API integration tests.
7. Update documentation indexes and `ROADMAP.md`, then run formatting, clippy,
   build, tests, purity, rustdoc, doctests, and line coverage.

## Completion checklist

- [x] Public records and serde forms are tested.
- [x] The log port is object-safe and runtime-neutral.
- [x] Projection and every page failure path are tested.
- [x] Briefing and initialization behavior are tested.
- [x] Documentation and roadmap describe the shipped boundary.
- [x] Full workspace verification passes.
