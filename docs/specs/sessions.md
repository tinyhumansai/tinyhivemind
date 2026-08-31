# Attributed session projection and initialization

**Status:** Implemented  
**Owner:** tinyteams maintainers

## Problem

Agents sharing a conversation must read a bounded, chronological transcript
without losing who authored each message. The host owns the durable log, so the
collaboration runtime needs a narrow read port and a deterministic paging walk
instead of another store. A newly initialized agent also needs an ephemeral
description of its teammates and the rules of the shared room.

## Goals

- Read a host-owned log through an object-safe, runtime-neutral port.
- Project a bounded channel or thread history while preserving attribution.
- Reject malformed pagination before it can duplicate, skip, or loop history.
- Return a separate deterministic team briefing for session initialization.
- Construct a conservative briefing from host-supplied desk and roster
  snapshots without introducing host role types.

## Non-goals

- Storage, model clients, HTTP, runtime handles, watermarks, responder
  selection, or mention dispatch.
- Persisting the team briefing or assigning it a sequence number.
- Interpreting a peer message as the viewer's own prior reply.

## Proposed behavior

The `tinyteams` runtime crate depends on and re-exports `tinyteams-core`.
`Sequence(u64)` identifies host log rows. `Conversation` names a desk by its
canonical id and display name and optionally names a thread root. `LogMessage`
contains its sequence, optional stored chat id, optional direct parent,
`SessionAuthor`, and untouched content. `SessionPage` is newest-first and may
provide `next_before`, an exclusive cursor for an older page. That cursor may
equal or be older than the page's oldest row, but cannot be newer.

`SessionLog::read_before` returns a boxed, sendable future and a boxed,
sendable source error. It is object-safe and does not select an async runtime.
`project_session` reads at most `SCAN_LIMIT` (2048) raw rows in pages of at most
`PAGE_SIZE` (512), then returns at most the requested window (normally
`SESSION_WINDOW`, 30) in chronological order. A zero window performs no read.
The query's `before` cursor is exclusive, so the current message can be
excluded by using its sequence.

Each page is validated before use. Rows must be strictly descending, below the
requested cursor, and unique across the walk. A nonempty page's next cursor
must be no newer than its oldest row; an empty page cannot advertise another
cursor. Relative to the requested cursor it must strictly move toward older
rows. Violations are typed errors. Source read errors retain their source.
Reaching the scan cap is successful and returns the qualifying history found
so far.

Conversation filtering uses `tinyteams_core::chat::same_conversation` against
both the desk id and display name. Channel projection keeps only rows without a
parent. Thread projection keeps the root and its direct children, and stops
scanning once the root row is reached, even if that row has blank content.
Content whose trimmed form is empty is skipped; all other content bytes and
every author are preserved unchanged.

`TeamBriefing` identifies the viewer and desk and lists `BriefedTeammate`
records. A teammate has an id, label, and optional role and description.
`from_snapshots` validates host-supplied desk and roster snapshots. General
uses the active roster; another desk uses its effective active desk order.
It excludes the viewer, retired and unknown ids, and duplicates. Because the
core snapshots carry no agent role or description, these optional fields are
unset; hosts may construct richer validated records directly.

`TeamBriefing::system_text` deterministically identifies the viewer and desk,
lists teammate `@id` handles with optional metadata, and states that peer
messages remain attributed and are not the viewer's replies. It also states
that `@everyone` is context only and never fan-out, and that mentions remain
context only until dispatch is introduced in P7. `initialize_session` returns
the briefing and projected history as separate values; the briefing is never
stored, sequenced, or counted against the history window.

## Invariants and constraints

- The host owns every durable row and cursor.
- The runtime owns no database, file, socket, transport, or model client.
- The core crate remains synchronous and runtime-free.
- One source row produces at most one attributed session message.
- Page validation prevents non-advancing walks and duplicate output.
- Briefing order is deterministic and follows effective desk or roster order.

## Acceptance criteria

- Tests cover zero-window reads, exclusive bounds, multi-page ordering,
  malformed pages, duplicate rows, non-advancing and empty-page cursors, the
  scan cap, desk filtering, channel and thread projection, root termination,
  blank-content skipping, current-message exclusion, and attribution.
- Tests cover briefing filtering, order, deterministic text, General behavior,
  initialization separation, and projection error propagation.
- Public payload serde forms, rustdoc examples, workspace contracts, purity,
  rustdoc, and doctests pass.

## Open questions

None for P4. Watermark-based continuous sharing begins in P5.
