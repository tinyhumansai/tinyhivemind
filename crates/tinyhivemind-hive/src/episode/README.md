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
validate roster + desks + policy
  └─ budget spent?                          → Exhausted
     └─ fold traces (above the watermark,
        and only from current desk members)
        └─ fold standings and, when `directory` is set, the directory,
           both at the same sequence; take consensus
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

### Who knows what

When `policy.directory` is `Some`, `step` folds a `Directory` from the same
traces at the same sequence as the standings, and hands it to the attention
market. The member the transcript says holds the contested topic — and has
taken no position on it — bids `BidReason::Knows`, between `Dissent` and
`Quiet`.

Nothing is stored. The directory dies with the step that folded it, so a wrong
estimate cannot follow a member into the next turn, let alone the next episode.
`policy.defer_cap` bounds a chain of `!defer` turns; `None` leaves deferral
promotion uncapped by `defer_cap` specifically, bounded only by `turn_budget`,
which is finite either way — `None` is uncapped, not off. `Some(0)` is
rejected as `Error::ZeroDeferCap` rather than read as a quieter way of turning
deferral off.

Both knobs are `None` in `EpisodePolicy::DEFAULT`, and both are
required-but-nullable on the wire, so a policy serialized before they existed
fails to decode rather than quietly acquiring a default. See
[`../directory/README.md`](../directory/README.md).

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
| `EpisodePolicy` | Budget, blind round, dominance and repetition caps, directory, defer cap, distance basis, quorum, weights. |
| `EpisodePolicy::for_room` | The policy a desk of *N* should carry, with every absolute bound scaled to it. |
| `EpisodeState` | Conversation, spend, phase, thresholds, watermark, commit boundary. |
| `HiveTurn` | The authorized turn, and the state to commit after it lands. |
| `HiveStep` | `Speak` \| `Converged` \| `Deadlocked` \| `Exhausted` \| `Idle`. `Exhausted` carries the standings the budget bought and the visibility it ended at. |
| `Phase`, `Visibility` | The two one-turn modes. |

## File layout

`mod.rs` holds `step`, `project_for`, and the private folds between them;
`types.rs` holds the stable `EpisodePolicy`, `EpisodeState`, `HiveTurn` and
`HiveStep` payloads. The unit suite lives under `test/`, one file per behavior
area rather than one 1,000-plus-line file:

| File | Covers |
| --- | --- |
| `test/support.rs` | Shared fixtures: the three-member `Room`, transcript builders, `run`/`speaking`. |
| `test/wire_forms.rs` | Serde pins for the policy, state, and tagged `HiveStep` variants. |
| `test/termination.rs` | The single-turn invariant and budget-bounded termination. |
| `test/quorum_and_convergence.rs` | Quorum, the one-way phase change, and the watermark. |
| `test/deadlock.rs` | Deadlock and cross-inhibition through the whole machine. |
| `test/turn_dynamics.rs` | Blind visibility and the threshold charge carried across turns. |
| `test/failure_paths.rs` | Malformed roster, desk, threshold, and policy inputs. |
| `test/expert_delegation.rs` | The directory and defer-cap knobs, on and off. |
| `test/off_floor_asides.rs` | An aside carries information, never support, for every reader alike. |
| `test/concurrent_asides.rs` | The concurrent-aside cost guarantee and its sequence-shift limit. |

Every submodule is a descendant of `episode`, so each can see the module's
private items exactly as the old flat `test.rs` could — nothing here changes
what a test may reach, only where it lives.

## Operational constraints

- **Commit `next_state` only after the turn is durably appended.** It is
  returned, never applied, exactly as `sharing::prepare_delta` requires. The
  caller owns the compare-and-swap.
- **`turn_budget` must be finite.** Termination is a property of the machine:
  `spent` strictly increases on every `Speak`, and the budget check runs before
  the increment, so the counter can never overflow.
- **`EpisodePolicy::DEFAULT` is wrong above about a dozen members, silently.**
  Three of its numbers are absolute where the quantity they bound scales with
  the room: `turn_budget: 12` is fewer turns than a room of thirteen has
  members, and under `blind_round` visibility lifts only once *every* member has
  authored, so such a room stays `Visibility::Blind` for its whole episode and
  never deliberates at all; `quorum.threshold: 2` is two supporters whether the
  desk holds five members or a thousand; `weights.half_life: 20` is twenty rows
  against an opening round that is `members` rows long. Use
  `EpisodePolicy::for_room`. `HiveStep::Exhausted` reports the first of the
  three when it happens, rather than leaving it to be inferred.
- **Exhaustion is diagnosable.** `Exhausted` carries the standings and the
  visibility the episode ended at, so "spent thirty turns and nearly carried two
  options", "spent thirty turns and deposited nothing", and "never saw itself"
  are three different reports rather than one. They are folded *before* the
  budget check for that reason, which costs one fold on the step that ends the
  episode.
- **Thresholds must name active desk members.** A stale threshold record for a
  retired agent is rejected rather than ignored, so a roster change cannot
  silently alter who gets the floor.
- **The directory is refolded, never carried.** It is not in `EpisodeState`,
  because an iterated per-turn update would not be commutative and stored state
  would have to be invalidated whenever the transcript is re-paged.
- **Nothing here uses floating point.** Every score is fixed-point integer, so
  the fold is reproducible and every payload derives `Eq`.
