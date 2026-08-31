# The episode

An episode is a bounded deliberation over one desk: a sequence of single turns
that ends when the room converges, deadlocks with nobody left to break the tie,
runs out of budget, or falls silent.

## Design

`step` is a fold. It takes the caller's transcript, roster, desk set and policy,
and returns one of five outcomes. It never appends, never waits, and never calls
back into the host — so the whole state machine is testable without a fixture,
an executor, or a mock.

```text
validate roster + desks
  └─ budget spent?                          → Exhausted
     └─ fold traces (above the watermark,
        and only from current desk members)
        └─ fold standings, take consensus
           ├─ Quorum & Commit & !commit recorded → Converged
           ├─ Quorum & Deliberate    → flip phase, emit one commit turn
           ├─ Deadlock & no free member         → Deadlocked
           └─ otherwise take bids
              ├─ a winner                       → Speak (exactly one)
              └─ nobody cleared threshold       → Idle
```

### Exactly one turn

`HiveStep::Speak` carries a single `HiveTurn`, and there is no variant that
carries two. The charter's *one message, one turn* rule is therefore a type
invariant rather than a convention, in the same way
`MentionDispatchDecision::One` already is. `floor_holder` taking the argmax —
rather than everyone above threshold — is what produces that single winner.

### Who may vote

Only messages *strictly above* `state.watermark`, authored by a **current
active member of the episode's desk**, are folded into traces. A retired agent,
or one from another desk, that posts into the room is context: visible to a
turn, but unable to manufacture a quorum nobody eligible actually holds.

### Convergence is recorded, not inferred

Reaching quorum flips the phase, fixes `commit_boundary` to the sequence
standings were just folded to, and buys one commit turn; the episode ends only
once a `!commit` trace names the carried topic at a sequence strictly after
`commit_boundary`. That boundary is what stops an unrelated `!commit` that
predates the commit turn — planted speculatively, or left from an earlier
exchange, and happening to share the carried topic — from being misread as
this turn's decision. If nobody records it after the boundary, the episode
runs on and stops at its budget — bounded either way, and never silently
converged on a decision nobody wrote down.

### The watermark

Only messages *strictly above* `state.watermark` are folded into traces.
Everything at or below it is context: visible to a turn, but unable to vote. An
episode therefore never inherits the quorum of the conversation that preceded
it, which is what lets several episodes run over one long-lived desk.

### The one-way phase change

Quorum flips `Deliberate` to `Commit` once, and nothing flips it back.
Deliberation and commitment are different classes of turn, and a room that has
settled must not reopen because a late trace arrived or because old support
decayed out of the window. Unawareness of termination conditions is one of the
most common observed multi-agent failures; this is the guard against it.

### Visibility, not concurrency

Independence — Surowiecki's condition that a shared transcript destroys, because
the third speaker reads the first two before answering — is bought here with
`Visibility::Blind` and `project_for`, not with parallel execution. During the
opening round, until every member has been heard once, a turn sees the operator,
system and person messages plus its own work, but not a peer's position.

That is the whole answer to "but a hive mind needs fan-out". See
[`../../../../docs/adr/0002-hive-episodes-are-sequential.md`](../../../../docs/adr/0002-hive-episodes-are-sequential.md).

## Public surface

| Item | Purpose |
| --- | --- |
| `step` | The fold. Returns exactly one outcome. |
| `project_for` | Filters a transcript to what one authorized turn may see. |
| `EpisodePolicy` | Budget, blind round, dominance and repetition caps, quorum, weights. |
| `EpisodeState` | Conversation, spend, phase, thresholds, watermark, commit boundary. |
| `HiveTurn` | The authorized turn, and the state to commit after it lands. |
| `HiveStep` | `Speak` \| `Converged` \| `Deadlocked` \| `Exhausted` \| `Idle`. |
| `Phase`, `Visibility` | The two one-turn modes. |

## Operational constraints

- **Commit `next_state` only after the turn is durably appended.** It is
  returned, never applied, exactly as `sharing::prepare_delta` requires. The
  caller owns the compare-and-swap.
- **`turn_budget` must be finite.** Termination is a property of the machine:
  `spent` strictly increases on every `Speak`, and the budget check runs before
  the increment, so the counter can never overflow.
- **Thresholds must name active desk members.** A stale threshold record for a
  retired agent is rejected rather than ignored, so a roster change cannot
  silently alter who gets the floor.
- **Nothing here uses floating point.** Every score is fixed-point integer, so
  the fold is reproducible and every payload derives `Eq`.
