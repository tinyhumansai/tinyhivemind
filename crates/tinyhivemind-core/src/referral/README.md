# Referral module

This module decides, from one committed agent reply, whether it may start
exactly one more turn that runs on a **different** conversation from the one
that triggered it, and how any answer produced there gets carried back. It
performs no IO and retains no state.

`crate::dispatch::mention_dispatch` answers *who replies next*, bound to the
conversation the trigger was committed on. This module answers the question a
second desk makes possible: *whose channel does the reply run in* — and, once
it has run there, *how does the answer come back*. Everything here is still
one message, one turn: a desk mention resolves to exactly one agent before it
leaves the fold, and `ReferralDecision` has no variant carrying two.

## Public surface

- `referral` is the entry point: given a policy, a `ReferralInput`, a roster,
  and a desk snapshot, it returns a `ReferralDecision`.
- `ReferralPolicy` is the explicit host policy — `enabled`, `max_hops`,
  `reach`, and `returns` — with `ReferralPolicy::DEFAULT` (and `Default`)
  referring nothing.
- `ReferralReach` is one knob, not two: `Local` never crosses,
  `Channels` lets a mentioned agent's own desk host the reply, and `Desks`
  additionally lets a nonquiet `@#desk` mention select that desk's one
  responder. The three widen strictly, so `Desks` without `Channels` is not a
  policy a host could hold.
- `ReferralInput` carries the committed reply's key, conversation, author,
  content, revalidated mentions, current hop, and the `ReferralOrigin` it is
  answering, if any.
- `ReferralDecision` is `None { reason: NoReferralReason }` or
  `One { referral: Box<Referral> }`.
- `Referral` is the canonical child-turn record: its `kind` is `Forward` or
  `Return`, and `Referral::crosses` reports whether `to != from`.

## Decision order

`referral` evaluates fail-closed and in order:

1. A disabled policy, an exhausted hop budget (`hop >= max_hops`), or an
   overflowed child hop refuses immediately.
2. An inactive author refuses.
3. The first reading-order nonquiet mention this policy may act on is the
   candidate — an agent mention always qualifies; a desk mention only when
   `reach` is `Desks`. Person and everyone mentions, and a policy with no
   qualifying mention, fall through to a possible return instead.
4. **Forward to an agent:** a self-mention, or a target that is not an active
   roster member, refuses. If `reach` does not cross, or the target is already
   an effective member of this conversation's desk, the turn stays here.
   Otherwise it relocates to the target's home desk — its first desk in
   snapshot order — or refuses `TargetDeskless` if it has none.
5. **Forward to a desk:** an unresolvable identity, the current desk itself, or
   a desk whose only eligible member is the author, all refuse. Otherwise the
   turn lands on that desk's first eligible member (not the author).
6. **Return:** considered only when there is no forward candidate at all.
   Needs `returns` enabled, an `origin` on the input, a different conversation
   than the one that asked, and an active, non-self asker. Otherwise refused.

A later mention is never used as a fallback once the first qualifying one has
been evaluated and refused. A referral that stays on the triggering
conversation carries no `origin`, because its answer is already visible where
it was asked; only a crossing forward carries one, so its reply knows where to
return to. A `Return` itself carries no `origin`, so a round trip cannot ring.

## Operational constraints

- `referral` validates the roster and desk snapshots first and fails closed
  on a structural error.
- `max_hops` is a host-owned finite `u32` with no smaller library cap; the
  hop arithmetic is checked. There is deliberately no bound on how many
  channels one desk may ask — bounding that width is the host's job, because
  only the host knows what a question costs it.

## Bounding the width, and what a bad bound costs

The division of labour above is right and it is also a trap, so the measured
answer belongs here. The obvious host bound — *every peer, once* — is not a
bound: it grows with the federation while the asking desk's turn budget stays
whatever its own size earned. Ten members earn thirty turns; fifty desks
present forty-nine peers to ask.

Measured on the benchmark's federation, that is a total collapse — 100% correct
at twelve desks, **0.0% at twenty-five and above**, every desk episode ending
`exhausted`. Bounding width at a stated constant of two restores 100% and
spends *less*: 1343 turns against 6624 at a hundred desks. See
[`docs/experiments/2026-09-10-hive-at-scale.md`](../../../../docs/experiments/2026-09-10-hive-at-scale.md).

Two rules follow:

1. **Bound width by a constant the host states, never by the number of peers.**
   A bound that scales with the federation prices nothing.
2. **Do not spend an authorized turn on the question.** A question is a model
   call, not a turn; charging it to the floor takes the budget out of the room
   that has to decide. This is
   [ADR 0012](../../../../docs/adr/0012-an-exchange-round-spends-model-calls-not-turns.md)
   one level up.

And one limit: a bounded question cannot recover an error the whole federation
shares, because the peer it asks holds the same error. Where every desk is
wrong about the same thing, what lifts it is a desk publishing its reading to
*every* channel — one model call rather than one per peer — which is a
different mechanism and not more of this one.
- With only `enabled` and `max_hops` set (`reach: Local`), the decision is
  exactly what `mention_dispatch` decides on the same conversation — asserted
  by test, not merely claimed.
- `NoReferralReason` renders a `Display` sentence an agent may repeat to a
  person, held to
  [ADR 0009](../../../../docs/adr/0009-a-refusal-renders-what-the-caller-already-holds.md):
  every reason that turns on another agent's existence, activity, or desk
  membership renders the shared `NO_AVAILABLE_TARGET` sentence
  (`NoReferralTarget`, `TargetInactive`, `EmptyDesk`, `TargetDeskless`); a
  reason about the caller's own request, its own desk, or the desk directory
  — which carries no per-viewer scoping — renders its own wording.
  `HopOverflow` and `HopLimitReached` share one sentence, and that sentence
  matches `NoDispatchReason::HopLimitReached` verbatim: one hop budget, one
  vocabulary.

The normative behavior is in
[`../../../../docs/specs/cross-desk-referral.md`](../../../../docs/specs/cross-desk-referral.md).
