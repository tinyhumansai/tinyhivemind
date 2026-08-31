# Continuous transcript sharing

**Status:** Implemented
**Owner:** tinyteams maintainers

## Problem

An initialized agent can remain bound to one conversation while teammates add
messages. Reusing only the initialization history therefore hides interleaved
peer work. The host owns the log and the durable per-agent state, so the
runtime needs a stateless way to plan an incremental read without creating a
second journal or advancing state before a turn accepts the result.

## Goals

- Represent the last accepted exclusive sequence watermark and already-present
  future rows in caller-owned state.
- Read every eligible attributed row between that watermark and the next turn.
- Detect conversation changes, unavailable history, excessive gaps, and
  regressing bounds without returning a partial state advance.
- Let hosts commit returned state only after the delta and current trigger are
  accepted by their session using their own serialization or compare-and-swap.

## Non-goals

- Storage, compare-and-swap, locks, model calls, response selection, dispatch,
  or a runtime choice.
- Assuming global sequence numbers are contiguous within one conversation.
- Persisting another message journal.

## Proposed behavior

`SharingState` contains a `Conversation`, an exclusive `watermark`, and a
bounded `BTreeSet<Sequence>` of messages already present above that watermark.
`initialized_state` creates it only after the P4 briefing, projected history,
and current trigger have been accepted by the host session.

`note_present` records a row the host has already accepted concurrently. A
sequence at or below the watermark is a no-op; a later sequence is inserted
idempotently. More than `PRESENT_SET_LIMIT` (64) distinct later sequences is a
typed error and leaves the state unchanged. A duplicate remains idempotent when
the set is exactly full. Deserialization rejects an oversized set, and public
operations reject an oversized manually constructed state before reading or
mutating anything. It never moves the watermark.

`SharingQuery` borrows the desired conversation, the host's current
conversation, and state, and supplies the next turn's exclusive `before`
sequence. `prepare_delta` is stateless. If either current value differs from
the desired conversation it returns `Reinitialize(ConversationChanged)`.
Desk equivalence recognizes all General aliases but otherwise requires an
exact canonical id match, and thread roots must match exactly. A regressing bound
is a typed error; an equal bound returns an empty delta and unchanged state.

Otherwise the walk reads newest-first through the P4 `SessionLog`, validates
each page under the same cursor rules, and counts every raw row toward
`SCAN_LIMIT`. It stops after observing any raw sequence at or below the old
watermark. Global gaps are permitted. Within `watermark < sequence < before`,
it keeps nonblank rows from the same channel or exact thread, preserves author
and content, and omits sequences in `present_above_watermark`. The result is
chronological. Its next state moves the watermark to `before` and prunes
present sequences at or below it while retaining later concurrent sequences.

Reaching the scan cap without crossing the watermark returns
`Reinitialize(GapTooLarge)`. Exhausting the available log while still above the
watermark returns `Reinitialize(WatermarkUnavailable)`. Read and page errors
propagate. Reinitialization and errors never expose a partial delta or advance.

The records are value payloads with deterministic serde forms. `SharingQuery`
is a borrowed call input rather than a wire payload and intentionally does not
implement serde.

## Invariants and constraints

- The host owns the log and all durable sharing state.
- The host commits `next_state` only after delta plus trigger acceptance,
  serialized or guarded by host compare-and-swap.
- A retry with the same accepted state is deterministic and cannot mutate it.
- Channel and thread histories never mix; General aliases denote one desk.
- One raw row yields at most one attributed delta message.

## Acceptance criteria

- Tests cover interleaved peer messages, attribution, exclusive bounds,
  concurrent future rows, idempotent presence tracking, overflow, sparse pages,
  General aliases, channel/thread separation, conversation changes, regression,
  empty deltas, both reinitialization reasons, retry safety, simulated CAS, and
  all reused P4 page/read failures.
- Public value serde forms, workspace contracts, purity, rustdoc, doctests, and
  per-file line coverage pass.

## Open questions

None for P5. Hosts choose their persistence and concurrency mechanism.
