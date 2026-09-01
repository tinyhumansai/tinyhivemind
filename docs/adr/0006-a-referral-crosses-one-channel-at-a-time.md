# 6. A referral crosses one channel at a time, and carries information rather than a vote

- **Status:** Accepted
- **Date:** 2026-09-01

## Context

Everything through P14 stops at the edge of one conversation. `mention_dispatch`
decides at most one child turn and binds it to the conversation the trigger was
committed on; a hive episode is scoped to one `Conversation`. An agent on the
payments desk that needs a fact only the platform desk holds can mention the
platform engineer, but that pulls the engineer into payments, away from the
transcript that holds the fact and the members who could corroborate it.

That is not only an ergonomic gap. A desk is a **correlation boundary**. Members
of one desk read the same transcript, work the same part of the system, and are
wrong about the same things, so their errors are correlated and no amount of
deliberating inside the channel cancels them. Averaging correlated error does
not remove it. Pooling across desks is the only operation that can, and a shared
medium that cannot cross a channel cannot buy it.

The simulated federation in `examples/bench` measures exactly this, and the
numbers are unambiguous: at a desk bias of +110 over three desks, a siloed
federation scores **0.2%**, because every desk converges confidently on its own
decoy and the three decoys do not agree. Merging every member into one desk —
removing the boundary rather than crossing it — scores **10.5%**, because a
larger room with three factions cannot reach quorum. Crossing the boundary
scores **77.5%**.

Three design questions had to be answered, and each had a tempting wrong answer.

**Does a desk mention wake the desk?** The obvious reading of `@#platform` is
*everyone on platform*. That is fan-out, it is the failure mode the charter
exists to prevent, and it would let one message start N turns.

**Does the answer come back by itself?** A referral that only goes outward makes
the asker poll, or makes the host remember the pairing — a second journal by
another name.

**Does support cross?** The cheapest way to make a federation converge is to let
a supporter on one desk count on another. It is also wrong: it is not pooling
information, it is voting twice.

## Decision

`referral` is a pure fold in `tinyhivemind-core`, alongside `mention_dispatch`
rather than replacing it, gated by a `ReferralPolicy` whose every flag is off by
default.

**One turn, one target, one channel.** A desk mention resolves to exactly one
agent — the addressed desk's first effective active member other than the author
— *before it leaves the fold*. There is no decision variant carrying two.
`direct_responder` is untouched, so a desk mention still cannot start a turn
through the ordinary responder ladder.

**The back edge is a referral too.** A crossing forward carries a
`ReferralOrigin`. A reply committed under one, with no forward candidate of its
own, yields exactly one `ReferralKind::Return` addressed to the asker on the
conversation that asked. A return carries no origin, so a round trip is two hops
and cannot ring.

**A crossing referral lands on the desk channel, never in a thread.** A thread
root is a sequence number in the conversation that owns it; carrying one across
would name a message that does not exist.

**What crosses is information.** The library takes no position on the content,
and the benchmark's members deposit `!evidence`, which adds no supporter to any
topic. The far desk hears another channel's reading, and its members still have
to spend their own turns before anything is counted.

**With the new knobs off, `referral` decides exactly what `mention_dispatch`
decides.** That is asserted by a test over every interesting input, not merely
documented.

## Consequences

- Hosts get a second, wider bound to own. `referral` bounds how *deep* a chain
  goes; nothing in the library bounds how *many* channels one desk may ask. The
  benchmark's host caps it at one question per peer desk, and the runtime
  `README` says plainly that this is the host's job.
- A crossing referral writes into a channel its author is not a member of, so
  the host's `ReferralQueue` transaction must authorize **both** conversations.
  Authorizing only the source desk authorizes nothing.
- The host must carry `ReferralOrigin` back into the next `ReferralInput` when
  it runs a referred turn. Nothing in the library remembers it — that would be
  state, and this crate holds none.
- Timing is now load-bearing in a way it was not before. A desk whose members
  share a bias reaches quorum inside its own blind opening round, so an answer
  arriving after that is information the desk has already voted past. The
  benchmark's first version asked *after* proposing and every desk committed to
  its decoy with the correction three lines below the decision. This is a host
  policy question the library cannot decide, and it is the single largest effect
  measured.
- An answer can be stranded: the desk that asked may finish first. The benchmark
  counts stranded answers rather than dropping them quietly, because they are
  compute the federation paid for and could not use.
- Turning this on is not free and is not always right. At `--bias 0` — desks
  with no blind spot of their own — crossing buys nothing and costs twice the
  turns. The mechanism is for correlated error across channels, and a host
  without that should leave `ReferralPolicy::DEFAULT` alone.
