# Responder selection

**Status:** Implemented
**Owner:** tinyhivemind maintainers

## Problem

A group message must start exactly one agent turn. The choice can be explicit,
desk-defined, or model-assisted, but hosts need one deterministic ladder and a
narrow selector boundary that cannot read tools, transcripts, or host state.

## Goals

- Select one active responder from mentions, desk policy, direct-agent chats,
  or the orchestrator fallback.
- Support lead-first and model-assisted desk modes.
- Keep the model call behind an object-safe, runtime-neutral `Selector` port.
- Fail closed on ambiguous agent names and malformed selector output.

## Non-goals

- Dispatching a turn, expanding `@everyone`, persistence, tools, or transcript
  access. P7 owns dispatch.
- Allowing a selector to choose an agent outside the effective desk roster.

## Proposed behavior

`Desk` gains `ResponderMode`: `lead` is the default and is omitted from its
wire form; `auto` opts a desk into model-assisted selection. Existing lead-mode
desk JSON therefore stays byte-for-byte structurally compatible.

`responder_plan` validates the borrowed snapshots and applies this ladder:

1. The first reading-order, nonquiet, active direct agent mention wins.
2. A non-General chat resolving to a desk uses that desk. Lead mode selects its
   first effective active member. Auto mode uses zero members as an
   orchestrator fallback, one member directly, and two or more as selector
   candidates when selection is allowed. Disabled selection chooses the first
   candidate at the desk-default rung with a `disabled` disposition.
3. A bare active agent id/name or `dm:<id-or-name>` selects that agent. Desk
   interpretation outranks an agent collision. Ambiguous agent names fail
   closed to the orchestrator.
4. General, unaddressed, and unresolved chats select the active orchestrator.

Effective desk candidates preserve desk order, exclude retired or absent
agents, and are deduplicated. Candidate detail is clamped to that set. Missing
detail uses the member id as its label and `Teammate` as its role. When an
allowed Auto desk with two or more candidates reaches candidate enrichment,
duplicate detail for an effective candidate id is a structural error. Extra
detail is ignored, and metadata is not validated on any rung that does not
construct a `SelectionRequest`.

All responder payload fields are required in their JSON wire form. The
`description` and `chat` fields accept explicit `null`; omission remains a
missing-field error so producer drift cannot silently change a request.

`SelectionRequest` exposes only the raw message, canonical desk id, and the
bounded candidate descriptions. `Selector` returns text. `accept_selection`
accepts only one candidate id, ASCII-case-insensitively, after trimming, one
matching quote/double-quote/backtick wrapper, and one trailing period. Empty,
prose, multiple ids, and out-of-set ids are invalid. Selector absence/failure
and invalid output deterministically choose the first candidate with the
corresponding disposition at the desk-default rung. Only accepted selector
output produces the auto-selection rung with a `selected` disposition.

## Invariants and constraints

- One request produces exactly one decision and never dispatches.
- Explicit direct mentions win even when the agent is outside the desk.
- Person, desk, everyone, and quiet mentions do not select a responder.
- The selector sees no transcript, tools, model handles, or host callbacks.
- The orchestrator is required to be active only if its fallback rung is
  reached; otherwise `NoActiveResponder` is returned.

## Acceptance criteria

- Desk lead and auto wire forms are pinned.
- Tests cover every ladder rung, collisions, inactive members, selector
  success/failure/absence/invalid output, parsing boundaries, and exactly-one
  decision for messages containing multiple direct mentions and `@everyone`.
- The runtime selector is called at most once and has no dispatch capability;
  P7 exposes dispatch separately behind the atomic host queue port.
- Core remains accepted by the purity assertion.

## Open questions

None for P6. Turn creation and hop bounds are deferred to P7.
