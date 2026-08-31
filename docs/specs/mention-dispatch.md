# Mention dispatch

**Status:** Implemented
**Owner:** tinyteams maintainers

## Problem

An agent reply can explicitly address another agent, but resolving the mention
does not safely start the next turn. Hosts need a single, idempotent dispatch
edge that cannot fan out, loop without a bound, or create a second journal.

## Goals

- Decide from committed reply data whether one direct agent mention may start
  one child turn.
- Bound chains with a finite, host-supplied `max_hops` and no library hard cap.
- Put durable, authorization-aware, exactly-once enqueueing behind one host
  port.
- Keep the feature explicitly disabled until a host opts in.

## Non-goals

- Fan-out for person, desk, or `@everyone` mentions.
- Library-owned persistence, retries, environment configuration, or model
  selection.
- OpenCompany integration or a live provider test; those follow this library
  phase.

## Proposed behavior

`MentionDispatchPolicy { enabled, max_hops }` is supplied for every decision.
Zero hops disables dispatch. There is no compiled-in ceiling: values through
`u32::MAX` are valid, while child-hop arithmetic is checked.

`mention_dispatch` evaluates in this order: disabled policy, exhausted hop
budget, inactive source, then the first reading-order nonquiet `Agent` mention.
Quiet, person, desk, and everyone mentions are skipped. If that first direct
agent mention is the source itself or names an inactive target, the decision
fails closed and does not try a later mention. Otherwise the decision contains
one canonical `MentionTurnRequest` whose child hop is the parent hop plus one.

Dispatch keys bind the committed trigger sequence. The request also binds the
conversation (`desk_id` and optional thread root), source id, target id,
content, and child hop. The runtime `MentionTurnQueue::enqueue_once` port
receives that single owned request and returns `Enqueued`, `Already`, or an
expected refusal (`Unauthorized`, `TargetUnavailable`, or `FeatureDisabled`).
`dispatch_mention` invokes it zero or one time, returns refusals as outcomes,
and returns unexpected host failure as a typed error with its source. It never
retries or considers another mention.

All public payload fields are required in JSON. Enum tags and variants use
snake case. A host maps its richer runtime conversation to the pure dispatch
conversation; this is a snapshot, not a host type or callback.

## Invariants and constraints

- One committed message creates at most one turn.
- The host owns storage and the only idempotency record.
- `enqueue_once` must atomically re-read and validate the committed reply,
  current policy, authorization, target availability, and conversation binding,
  then durably enqueue at most once under the conversation-and-trigger key.
- The library never reads environment variables or enables the feature through
  a Cargo feature. OpenCompany's eventual adapter default is two hops, but its
  first integration remains disabled until deliberately enabled.
- Host refusal is final for the call; there is no library retry or fallback.

## Acceptance criteria

- Pure tests pin wire forms and cover disabled/zero, hop limits 1 and 2, a
  large limit, `u32::MAX`, inactive/self targets, reading order, quiet and
  non-agent mentions, and exactly-one decisions.
- Runtime tests prove zero-or-one queue calls, refusal mapping, source-preserved
  host failure, bound-scope keys, and concurrent duplicate enqueue producing
  one `enqueued` and one `already` result.
- The queue contract documents the required atomic host transaction.
- Host integration tests revalidate stored rows and transaction rollback.
  A later live test proves two agents exchange at least one attributed turn
  through a real provider; neither is simulated in this crate.

## Open questions

None for the library phase. Host integration and live verification are next.
