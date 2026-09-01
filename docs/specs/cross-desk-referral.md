# Cross-desk referral

**Status:** Accepted
**Owner:** tinyhivemind maintainers

## Problem

Every mechanism in this workspace stops at the edge of one conversation.
`mention_dispatch` decides at most one child turn and binds it to the
conversation the trigger was committed on — the doc comment on
`MentionDispatchInput::conversation` says so outright: *"Conversation to which
any child turn remains bound."* A hive episode is likewise scoped to one
`Conversation`.

So an agent on the `payments` desk that needs a fact only the `platform` desk
holds has no move. It can mention the platform engineer, but that pulls the
engineer into the payments channel, away from the desk whose transcript holds
the fact and whose members could corroborate it. There is no way to *ask a
channel a question and get an answer back*, which is the ordinary way a
organisation of more than one team solves anything.

This matters beyond ergonomics. A desk is a correlation boundary: members of one
desk read the same transcript and share the same framing, so their errors are
correlated and no amount of within-desk deliberation cancels them. Pooling
across desks is the only operation that can. A shared medium that cannot cross a
channel cannot buy that.

## Goals

- Decide, from committed reply data alone, at most one child turn that may land
  on a **different** conversation from the one that triggered it.
- Carry the answer back: a reply produced under a referral may return exactly
  one turn to the conversation that asked.
- Keep the whole decision a pure fold, bounded by a host-supplied finite hop
  budget, with no library cap and no fan-out.
- Preserve today's `mention_dispatch` behaviour exactly when the new
  cross-channel knobs are off.

## Non-goals

- Broadcast to a desk. A desk mention selects one responder, never N.
- A second journal. A referral names conversations the host already owns and
  stores nothing.
- Delivery, retry, or ordering. The host's queue owns all of that.
- Cross-desk *episodes*. An episode stays scoped to one conversation; desks
  exchange messages, not state.

## Proposed behavior

### Policy

```rust
pub struct ReferralPolicy {
    pub enabled: bool,
    pub max_hops: u32,
    pub cross_desk: bool,
    pub desk_mentions: bool,
    pub returns: bool,
}
```

`ReferralPolicy::DEFAULT` has every flag `false` and `max_hops: 0`, so a host
that constructs one by default gets no referrals at all. With `enabled: true`
and every other flag false, `referral` decides exactly what `mention_dispatch`
decides, on the same conversation: that equivalence is the compatibility
statement, and it is asserted by test.

- `cross_desk` — a referral whose target is not an effective active member of
  the triggering desk relocates to that target's **home desk** instead of
  dragging them into this one.
- `desk_mentions` — a nonquiet `@#desk` mention becomes a candidate. It still
  selects exactly one agent: the addressed desk's first effective active member
  other than the author. `direct_responder` is untouched; a desk mention still
  cannot start a turn through the ordinary responder ladder.
- `returns` — a reply committed under a forward referral may carry exactly one
  answer back to the asker, on the conversation that asked.

### The fold

`referral(policy, input, roster, desks) -> ReferralDecision` evaluates in a
fixed order, and each rung is checked before the next:

1. a disabled policy returns `Disabled`;
2. `input.hop >= policy.max_hops` returns `HopLimitReached`;
3. the roster and desk snapshots are validated;
4. an inactive author returns `SourceInactive`;
5. a **forward** candidate is sought: the lowest-offset nonquiet mention that is
   an `Agent`, or a `Desk` when `desk_mentions` is on. Person and everyone
   mentions are skipped rather than treated as stopping conditions, exactly as
   `mention_dispatch` skips them;
6. if there is no forward candidate, a **return** is considered, and if there is
   no return either the decision is `NoReferralTarget`;
7. resolution of the chosen candidate fails closed. A self mention returns
   `SelfMention`, an inactive target `TargetInactive`, a desk mention naming the
   triggering desk `SelfDesk`, an addressed desk with no eligible member
   `EmptyDesk`, and a cross-desk target belonging to no desk `TargetDeskless`.
   A later mention is never used as a fallback.

Home desk is the first desk in `DeskSet` order holding the target as an
effective active member — deterministic, and stable under any host that hands
its desks over in a stable order.

### Where a referral lands

| trigger | `cross_desk` | conversation of the child turn |
| --- | --- | --- |
| agent on this desk | either | this conversation, thread root preserved |
| agent not on this desk | off | this conversation, thread root preserved |
| agent not on this desk | on | that agent's home desk, **desk channel** |
| desk mention | on | that desk, **desk channel** |
| return | on | the origin conversation, thread root preserved |

A referral that crosses a conversation always lands on the target desk's desk
channel, never in a thread. A thread root is a sequence number in the
conversation that owns it and means nothing in another; carrying one across
would name a message that does not exist.

### The back edge

A forward referral that crosses carries a `ReferralOrigin { conversation,
asker_id }`. When the host runs that child turn, it passes the origin back in
the next `ReferralInput`. A reply committed with an origin, no forward candidate
of its own, `returns` on, and an origin conversation different from the one it
was committed on yields `ReferralKind::Return` — one turn, addressed to the
asker, on the conversation that asked. A return carries no origin of its own, so
a round trip is two hops and cannot ring.

A forward that does not cross carries no origin: the answer is already visible
to the asker, because it was appended to the conversation the asker is reading.

### The runtime edge

`crates/tinyhivemind` adds a `ReferralQueue` port with one method,
`enqueue_once(Referral)`, and `dispatch_referral`, mirroring `MentionTurnQueue`
and `dispatch_mention` exactly: a pure `None` decision calls the queue zero
times, a `One` decision calls it exactly once, expected refusals come back as
outcomes, and an unexpected host failure becomes `Error::Enqueue` with its
source preserved. The port is the only idempotency boundary, keyed by the
committed trigger sequence and the bound *origin* conversation.

## Invariants and constraints

- One trigger yields at most one child turn. There is no decision variant that
  carries two, and a desk mention resolves to one agent before it leaves the
  fold.
- A referral never targets its author.
- `child_hop == hop + 1`, checked; overflow returns `HopOverflow`.
- A referral that crosses a conversation has `thread_root: None`.
- With `cross_desk`, `desk_mentions` and `returns` all off, `referral` and
  `mention_dispatch` agree on target, content, conversation and child hop for
  every input.
- Every payload is `Eq`, `serde`-round-trips, and pins its wire form in a test.
- The fold is pure: no clock, no IO, no host type.

## Acceptance

- The equivalence with `mention_dispatch` is a test, not a claim.
- A simulated swarm of several desks, each holding a distinct correlated bias,
  is measured against a siloed control that cannot cross a channel and against a
  single merged desk holding every agent. The mechanism is allowed to lose, and
  if it does the result is written up rather than buried — the precedent is
  `docs/experiments/2026-09-01-refutation-and-grounds.md`.
- A live run drives the same swarm through a real agent CLI and records what the
  agents actually did with the channel boundary.
