# Concurrent rounds

**Status:** Accepted
**Owner:** tinyhivemind maintainers
- **Decision:** [ADR 0014](../adr/0014-a-round-authorizes-concurrent-turns.md),
  superseding [ADR 0002](../adr/0002-hive-episodes-are-sequential.md)
- **Evidence:** [`../experiments/2026-09-09-topology-at-scale.md`](../experiments/2026-09-09-topology-at-scale.md)

## Problem

An episode is a bounded sequence of single turns. `HiveStep::Speak` carries
exactly one `HiveTurn`, and [ADR 0002](../adr/0002-hive-episodes-are-sequential.md)
made that a type invariant on purpose.

That ADR named the condition under which it would be reversed:

> If a future host demonstrates a decomposable workload where sequential
> episodes are the bottleneck, reversing this needs a new ADR and a new type —
> not a quiet relaxation of `HiveStep::Speak`.

[`../experiments/2026-09-09-topology-at-scale.md`](../experiments/2026-09-09-topology-at-scale.md)
is that demonstration. On a hidden profile, 1000 rooms per size:

```text
arm                 3        5        8       16       32       64
vote             31.2     15.9      7.4      1.1      0.0      0.0
hive+            31.3     16.0      8.7      1.0      0.0      0.0
hive+rounds      31.3     16.1      9.1      1.1      0.0      0.0
hive+pooled      95.7     91.9     87.0     85.2     87.4     89.8
```

Every floor-bound arm reads **0.0% by thirty-two members**. `hive+pooled` —
every reading in every member's hands at once — is flat at 85–96% across the
whole ladder. **The information is never lost. Every mechanism for moving it
through a serial floor fails**, and it fails because turns per member is
`turn_budget / n`: the supporters a topic needs grows with the room while each
member's chances to supply one do not.

The same table prices the cost wrongly, and for the same reason. `hive+` spends
**65.1 turns** at sixty-four members and `vote` spends **192**. Those are
comparable only if turns are serial. They are not: a seat is an async session,
`vote`'s 192 turns are *one round* of 192 parallel calls, and `hive+`'s 65.1 are
sixty-five sequential ones. The published `hive+ − vote: +3.6 [+2.1, +5.0]` buys
3.6 points for roughly seven times the wall clock, and no column in the harness
can show it.

## Goals

- Let an episode authorize a **set** of turns that run concurrently, so an
  episode's depth is its number of rounds rather than its number of turns.
- Preserve every counting guarantee: the decision a concurrent round reaches
  must be identical to the decision the same rows reach arriving one at a time.
- Keep the independence the blind round buys, and extend it: members writing
  simultaneously cannot read each other, so concurrency *is* a visibility
  filter rather than a threat to one.
- Keep fan-out **bounded**. A finite, policy-set width, knowable before the
  episode runs.
- Keep the crate pure: a fold over arguments the caller holds, no port, no
  waiting, no async runtime.

## Non-goals

- **Unbounded fan-out.** The danger charter rule 3 was guarding was N turns with
  no approval in sight, not concurrency. A round has a finite width and
  [`approval.md`](approval.md) gates crossing it.
- **A concurrent commit.** The room records one decision, so `Phase::Commit`
  authorizes a round of width one.
- **A second journal.** Rows in a round are ordinary rows in the one log.
- **Deciding the host's scheduling.** The library says *who may speak now*; the
  host decides whether to actually run them in parallel, in series, or at all.
- **Speculative or branching episodes.** A round is one set of turns on one
  transcript, not a tree.

## Proposed behavior

### A round

`step` returns the members authorized to speak **now**, together with the single
state the episode takes once every one of them has been appended.

```rust
pub enum HiveStep {
    /// The members authorized to speak now, and the state after all of them.
    Speak {
        /// Non-empty, in desk order, at most `policy.round_width` long.
        turns: Vec<HiveTurn>,
        /// State to commit once every turn is durably appended.
        next_state: Box<EpisodeState>,
    },
    Converged { .. },
    Deadlocked { .. },
    Exhausted { .. },
    Idle,
}

pub struct HiveTurn {
    pub agent_id: String,
    pub phase: Phase,
    pub visibility: Visibility,
    pub reason: BidReason,
    /// Exclusive lower bound: the sequence the episode opened at.
    pub watermark: Sequence,
    /// Rows a peer authored above this are concurrent with this turn, and
    /// are not visible to it.
    pub round_start: Sequence,
}
```

`next_state` moves from the turn to the round because it *is* a property of the
round: `spent` advances by the number of turns authorized, and `thresholds` is
charged for every one of their speakers. A turn is what one member may do; the
state is what the episode becomes, once.

A host may run fewer turns than it was handed — a seat may be unavailable — but
it commits `next_state` only when it has appended all of them. Running a subset
and committing the round's state would charge a threshold nobody spent.

### Who is in a round

`attention::bids` is unchanged. Selection changes from an argmax to the **top
`round_width` bids**, in descending urge, ties broken by desk order — which is
exactly what `floor_holder`'s strict `>` already does for the single winner, so
width one reproduces it.

A bid must still clear its own threshold to be in a round at all. A round is
therefore *at most* `round_width` wide and is often narrower; `Idle` is a round
of zero.

### What a turn in a round can see

`Visibility` gains no variant. The filter in `project_for` widens by one clause:
a peer agent's row authored **above `round_start`** is withheld, because it was
written concurrently with this turn and this turn cannot have read it.

At `round_width: 1`, `round_start` is the sequence standings were folded at, no
peer row exists above it when the turn composes, and the clause is a no-op. That
is what makes width one bit-identical rather than merely equivalent.

This is the same mechanism [ADR 0005](../adr/0005-a-blind-round-may-be-concurrent.md)
accepted for the opening round, generalized: **a concurrent round is a blind
round**, and independence is preserved by construction rather than by a flag.

### The width

```rust
pub struct EpisodePolicy {
    /// Turns one round may authorize. Finite; `1` is the sequential episode.
    pub round_width: u32,
    // ... unchanged
}
```

`round_width: 0` is `Error::ZeroRoundWidth` at step time, on the `defer_cap`
precedent: it would cap the mechanism before anyone could use it, which is a
configuration error rather than a quieter way of switching it off.

`EpisodePolicy::DEFAULT.round_width` is **not** 1. Seats are async sessions and
the default should say so; the benchmark arms measure what it costs.

### What does not change

- `quorum::standings`, `attention::bids` and `directory::directory` are already
  sorted, deduped, commutative and idempotent over `(sequence, offset)`. A batch
  of simultaneous rows folds identically to the same rows arriving singly. No
  fold is touched.
- Cross-inhibition, refutation, deferral, the directory and the exchange fold
  are unchanged. An `!object >N` cannot name a row from its own round because
  that row is not visible to it, which is the correct behaviour rather than a
  restriction.
- `Phase::Commit` authorizes width one.
- `turn_budget` still bounds total turns, so an episode still terminates. A
  wider round spends the budget faster in fewer rounds, which is the trade.

## Acceptance criteria

1. `round_width: 1` reproduces every recorded benchmark number bit-for-bit:
   `ladder 57.6 · vote 78.5 · hive 73.3 · hive+ 82.1` at 5000 rooms, and the
   scale table above.
2. **Round ≡ sequence.** For an arbitrary transcript and policy, the standings,
   consensus and directory folded over a round's rows are identical to those
   folded over the same rows appended one at a time. Asserted in
   `crates/tinyhivemind-hive/tests/fuzz_invariants.rs`, red before the change.
3. A round is never wider than `round_width`, never empty when `Speak`, and
   never contains the same member twice.
4. `next_state.spent` equals `state.spent + turns.len()`, and every speaker in
   the round is charged exactly once.
5. `round_width: 0` returns `Error::ZeroRoundWidth`.
6. A turn in a round cannot see a peer row authored above `round_start`.
7. `Phase::Commit` authorizes exactly one turn regardless of `round_width`.
8. `.github/scripts/assert-pure.sh` still passes: no runtime, no port, no
   dependency added.
9. The benchmark reports `rounds/ep` and `calls/ep` beside `turns/ep`, and the
   scale ladder is re-scored in them.
