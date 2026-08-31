# Mention module

This module turns authored `@` spans into pure addressing decisions. It has no
storage and never starts a turn. A host supplies one message body, its current
roster and desk snapshots, and optionally the mentions it stored with the
message.

## Public surface

- `Mention`, `MentionTarget`, and `MentionAuthor` are the stable wire records.
- `resolve` extracts mentions or revalidates an authoritative supplied list.
- `direct_responder` selects the first active agent ping without dispatching.
- `mentioned_members` expands targets into deduplicated context member ids.
- `MENTION_CAP` limits pings while preserving later mentions as quiet context.

`Person` rather than `User` is used at this boundary because the crate cannot
name a host identity type. Hosts project their human participants into the
neutral roster record.

## Resolution pipeline

1. Validate structural roster and desk invariants. Invalid snapshots fail
   closed.
2. Build current aliases for active agents, people, desks, and everyone.
3. Mask closed inline code spans and fenced code blocks without changing body
   offsets.
4. Extract authored spans, or validate supplied spans when the host has already
   parsed them.
5. Sort in reading order, remove self and duplicate-offset entries, quiet
   repeated targets, and enforce the ping cap.

Aliases are compared case-insensitively and longest-first. A collision across
different targets is intentionally unresolved; `@#` restricts the candidate
set to desks so a human or agent display name cannot shadow explicit desk
addressing. Person labels also receive deterministic ASCII slugs, with `_2`,
`_3`, and later suffixes assigned in roster order when base slugs collide.

Supplied metadata is authoritative about whether extraction should happen, but
it is not trusted as a routing bypass. Out-of-bounds, non-boundary, code-span,
and non-mention-shaped records are dropped. Structurally sound stale or
wrong-current-alias records remain visible as quiet context.

## Operational constraints

- All offsets are UTF-8 byte offsets into the original body.
- Unknown and ambiguous references fail closed rather than returning an error.
- Desk and everyone targets add context only; they do not select a responder.
- Exactly one agent target can be selected by `direct_responder`.
- Nothing in this module performs IO, awaits, calls a model, or dispatches.

The normative behavior is in
[`../../../../docs/specs/mentions.md`](../../../../docs/specs/mentions.md), and
the test-first implementation sequence is in
[`../../../../docs/plans/mentions.md`](../../../../docs/plans/mentions.md).
