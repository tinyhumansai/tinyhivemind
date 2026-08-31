# Roster and mention resolution

**Status:** Implemented
**Owner:** tinyteams maintainers

## Problem

A shared agent conversation needs one deterministic interpretation of authored
mentions. Hosts currently have to improvise alias matching, code-span handling,
and routing, which makes the same message address different teammates in
different surfaces. The pure collaboration layer must answer who a mention
means without dispatching a turn or consulting host state.

## Goals

- Represent active agents and signed-in people without host-specific types.
- Extract `@` and desk-only `@#` mentions while preserving authored spans and
  UTF-8 byte offsets.
- Revalidate supplied mentions against the current roster and desks.
- Normalize mentions for safe one-turn routing and context assembly.
- Pin the records' serde wire representation.

## Non-goals

- Starting agent turns, broadcasting, persistence, model calls, or IO.
- Treating a person as a host `User`; `Person` is intentionally neutral at the
  crate boundary and can represent any human participant supplied by a host.
- Validating references in stored mentions as structural roster failures.

## Proposed behavior

`RosterMember { id, name }` and `Person { id, label }` form a borrowed `Roster`.
Construction borrows member, person, and retired-member snapshots. Validation
rejects blank or duplicate ids within either namespace; aliases may collide,
because ambiguous aliases fail closed during resolution.

`MentionTarget` is a tagged union of `Agent { id }`, `Person { id }`,
`Desk { id }`, and `Everyone`. A `Mention` stores the exact authored `text`, its
UTF-8 byte `offset`, its target, and a `quiet` flag omitted from JSON when false.
`MentionAuthor` identifies an agent, a person, or an unclassified author.

`resolve(body, supplied, author, roster, desks)` has two input modes:

- `None` extracts mentions from the body.
- `Some` treats the supplied list as authoritative. An empty list suppresses
  extraction. Malformed spans are dropped. Well-formed but stale targets are
  retained and made quiet.

An extracted mention opens at the start of the body or after ASCII whitespace
or one of `([{`. `@#` restricts lookup to desks. The alias begins with a Unicode
alphanumeric character or `_` and ends at the body end, whitespace, or one of
`,;.?!:)]}'\"`. Current aliases are tried longest-first, case-insensitively.
Aliases include agent ids and nonempty names, person labels and stable ASCII
label slugs, desk ids and names, and `everyone`, `channel`, and `here`.
Ambiguity across distinct targets resolves nothing; `@#` deliberately bypasses
non-desk collisions. Candidate spans are tokenized on authored character
boundaries before case-insensitive comparison, so Unicode case mappings never
change or corrupt their byte offsets.

Closed inline-backtick spans and CommonMark-style fenced spans opened by three
or more backticks or tildes are ignored. An unclosed inline backtick masks
nothing. All offsets remain offsets in the original UTF-8 body.

Normalization sorts by reading order, keeps the first item at an offset, drops
self-mentions, makes repeated targets quiet, and allows at most
`MENTION_CAP` (50) nonquiet mentions. A supplied mention must have an in-bounds
character-boundary span outside code whose text is exactly one complete mention
token. A current alias yields its current target; an unknown, stale, retired,
or wrong-current-alias target is retained quiet.

`direct_responder` returns the first reading-order, nonquiet, active agent
target. Person, desk, and everyone targets never select a responder.
`mentioned_members` expands agent, desk, and everyone targets to active roster
members, deduplicates in reading order, and excludes the chosen responder.
Everyone means the addressed desk's members, or the full active roster for an
unaddressed/General conversation.

## Invariants and constraints

- Every operation is a pure fold over borrowed input.
- Agent and person id namespaces validate independently.
- Unknown or ambiguous mention input fails closed, never with a routing error.
- Resolution never dispatches and `Everyone` never fans out turns.
- One target can ping at most once per message; excess mentions remain context
  as quiet mentions.

## Acceptance criteria

- Exact serde shapes are pinned for every public payload.
- Tests cover boundaries, punctuation, Unicode offsets, aliases and ambiguity,
  desk-only syntax, inline/fenced code, supplied suppression and revalidation,
  self/repeat/cap normalization, retired agents, responder selection, and
  addressed-desk/everyone context expansion.
- `tinyteams-core` remains accepted by the purity assertion.

## Open questions

None for P3. Dispatch policy is deliberately deferred to P7.
