# 14. A round authorizes concurrent turns, and width is the bounded knob

- **Status:** Accepted
- **Date:** 2026-09-09
- **Supersedes:** [ADR 0002](0002-hive-episodes-are-sequential.md)
- **Generalizes:** [ADR 0005](0005-a-blind-round-may-be-concurrent.md)

## Context

[ADR 0002](0002-hive-episodes-are-sequential.md) made *one message, one turn* a
type invariant: `HiveStep::Speak` carries exactly one `HiveTurn`, and
independence is bought as a visibility filter rather than as concurrency. It
closed by naming the condition under which it would be reversed:

> If a future host demonstrates a decomposable workload where sequential
> episodes are the bottleneck, reversing this needs a new ADR and a new type —
> not a quiet relaxation of `HiveStep::Speak`.

[`../experiments/2026-09-09-topology-at-scale.md`](../experiments/2026-09-09-topology-at-scale.md)
is that demonstration, and it is unusually clean because it also rules out the
alternative explanation. On a hidden profile, every floor-bound arm — `vote`,
`hive+`, on-floor pairwise, and continuous off-floor exchange — reads **0.0% by
thirty-two members**, while `hive+pooled`, which hands every member everything
at once for free, is **flat at 85–96% across the whole ladder from three to
sixty-four**. Nothing is lost as the room grows. What fails is every mechanism
for moving information through a floor that admits one writer at a time, because
turns per member is `turn_budget / n` while the supporters a topic needs grows
with `n`.

The same experiment prices the cost in a unit that presumes the defect. `hive+`
spends 65.1 turns at sixty-four members against `vote`'s 192. Those are
comparable only if a turn is serial, and a seat is an async session: `vote`'s 192
turns are *one* round, `hive+`'s 65.1 are sixty-five. The recorded
`hive+ − vote: +3.6 [+2.1, +5.0]` is 3.6 points of accuracy for roughly seven
times the wall clock, and the harness has no column that can say so.

Three things ADR 0002 relied on have also turned out to point the other way.

**The conformity argument was about sequence, not about breadth.** Conformity in
LLM groups rises with *interaction time* and with sight of a peer's position.
Members who write simultaneously cannot read each other, so a wide round is
lower-contamination than the same turns taken in series, not higher. This
repository's own largest measured effect agrees: the blind round is worth **24
points** (82.1% against 58.0% at full visibility). A concurrent round is a blind
round, so concurrency should raise accuracy *and* cut depth.

**The sparse-topology results are about who reaches whom, not about how many
speak at once.** A round of eight members each writing one row to a shared floor
is not a fully connected graph; it is the same broadcast topology, one step
deeper per unit of wall clock.

**The folds were built for it already.** `quorum::standings`, `attention::bids`
and `directory::directory` sort and deduplicate by `(sequence, offset)` and are
commutative and idempotent by construction, specifically so a medium may
redeliver or reorder. A batch of simultaneous rows folds identically to the same
rows arriving singly. `ADR 0005` accepted concurrency for the opening round, and
`ExchangeRound::Open { members: Vec<String> }` already authorizes a whole batch
to write at once. The crate half-admitted this before the evidence arrived.

## Decision

An episode step authorizes a **round**: a set of turns that may run
concurrently, together with the single `EpisodeState` the episode takes once all
of them are appended.

```rust
Speak { turns: Vec<HiveTurn>, next_state: Box<EpisodeState> }
```

`next_state` moves off the turn and onto the round because it is a property of
the round — `spent` advances by the round's width, `thresholds` is charged for
every speaker in it. A turn is what one member may do; the state is what the
episode becomes, once.

Selection changes from `floor_holder`'s argmax to the top `round_width` bids in
descending urge, ties broken by desk order, each still required to clear its own
threshold. `EpisodePolicy::round_width` is finite, `0` is an error on the
`defer_cap` precedent, and `1` reproduces the sequential episode bit-for-bit.

Independence stays a visibility filter, now stated in terms of the round: a peer
row authored above `round_start` was written concurrently with this turn and is
withheld from it. At width one no such row exists and the clause is inert.

**Charter rule 3 changes with this ADR, in the same commit.** *One message, one
turn* becomes *one message, one round, of bounded width* — the bound, not the
serialization, is what was actually protecting anything.

## Consequences

- **Wall-clock latency of an episode is the depth of its rounds, not the sum of
  its turns.** That was listed as a consequence of ADR 0002 and is the thing
  being reversed. The benchmark gains `rounds/ep` and `calls/ep` as primary cost
  columns and re-scores every arm, which changes published numbers; that
  re-baselining is its own recorded experiment.
- **Fan-out is bounded rather than forbidden.** `round_width` is finite and
  knowable before the episode runs, and `docs/specs/approval.md` gates crossing
  it. A host cannot wake N agents by accident, which is what the removed rule
  was for.
- **The crate stays pure.** `step` remains a fold over arguments the caller
  holds; it returns a `Vec` and the host does the awaiting. No port, no async
  runtime, no dependency. `.github/scripts/assert-pure.sh` still passes.
- **The commit stays serial.** The room records one decision, so `Phase::Commit`
  authorizes a round of one. Widening that would let two members record
  different decisions for the same episode.
- **An `!object >N` cannot name a row from its own round**, because that row is
  not visible to its author. That is correct rather than restrictive: an
  objection is a response, and there is nothing yet to respond to.
- **The wire form of `HiveStep::Speak` changes.** A serialized step written
  before this ADR fails to decode rather than quietly acquiring a default,
  which is the convention `directory` and `defer_cap` already set.
- If concurrency turns out to buy depth at the cost of accuracy, the arms will
  say so, and `round_width: 1` remains a supported configuration rather than a
  deprecated one.
