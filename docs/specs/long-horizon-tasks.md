# Long-horizon tasks

**Status:** Accepted — implemented; the result is in
[`../experiments/2026-09-09-the-long-horizon.md`](../experiments/2026-09-09-the-long-horizon.md)
**Owner:** tinyhivemind maintainers
**Reading:** [`../research/multi-context.md`](../research/multi-context.md),
[`../research/long-context.md`](../research/long-context.md)
**Evidence:** [`../experiments/2026-09-08-pe1006-tool-room.md`](../experiments/2026-09-08-pe1006-tool-room.md)

## Problem

The benchmark measures **one categorical choice**. A room of five picks between
four options in about seven turns, and `--scale-sweep` grows the *room* to
sixty-four members without ever growing the *task*.

So the claim the library exists to test cannot be stated in it. That claim is:

> A single agent with context compaction wins at small scale and short horizon.
> A room of agents should win as the work gets longer and more complex.

Everything on the right-hand side of that sentence is unmeasurable here.
Nothing compounds — a room that answers wrong pays once, and there is no
stage two for a wrong stage one to poison. Nothing accumulates — the window
model in [`context.rs`] is real, but the information in an episode tops out at
about seventeen rows at the `hive+pooled` ceiling, so a window never binds the
way it binds in practice: run 28 of the PE 1006 desk spent **603,835 tokens
across twelve turns**.

And the control the claim is actually about **does not exist as an arm**.
`ladder` is one turn and `vote` is N independent one-shots. Neither is *one
agent working a long task, compacting as it goes*, which is the thing said to
win at small scale.

[`context.rs`]: ../../crates/tinyhivemind-hive/examples/bench/context.rs

## Goals

- A task with a **horizon**: several sub-decisions in sequence, on one
  accumulating transcript, where an early wrong answer makes the later ones
  harder rather than merely leaving them unaffected.
- A **solo baseline** that is handed everything the room collectively holds and
  must carry it in one window — the honest form of "a single agent with
  compaction".
- Compaction modelled as something better than dropping rows, so the room is
  not measured against a strawman.
- One stage reproduces the existing single-shot benchmark exactly.

## Non-goals

- **A new library mechanism.** This is a task shape and two arms in the
  harness. `tinyhivemind-hive` is unchanged.
- **A claim about wall clock.** Stages are counted, not timed.
- **Real work.** The participants stay arithmetic, for the reason they always
  have: a model would confound protocol quality with model quality.

## Proposed behavior

### The chain

`--stages N` runs **N sub-decisions in sequence**, decided by the same members.

- Each stage draws its own options, its own genuinely-best answer, and its own
  private evaluations. Stage *k*'s options are **named distinctly** from every
  other stage's, so a reading taken at stage 1 can never be scored against
  stage 2 — it can only take up room.
- **Context accumulates.** What a member was told at stage *k* is still in its
  window at stage *k+1*, occupying rows, whether or not it is still relevant.
  This is the only thing that makes a horizon cost anything.
- **A wrong stage poisons the next.** If the room decided stage *k* wrongly,
  every member's reading of stage *k+1*'s decoy is lifted by `POISON_LIFT`:
  the room is now reasoning from a false premise, and the option consistent
  with that premise looks better than it is. Recovery is possible and
  deliberately hard.
- Scored two ways: **`end-to-end %`**, every stage right, which is the headline
  because compounding is the point; and **`stage %`**, the per-stage rate.

At `--stages 1` there is no accumulation and no poisoning, and the run is
bit-identical to the single-shot benchmark.

### The arms

| arm | what it is |
| --- | --- |
| `solo` | **The missing control.** One agent handed every member's readings and every member's facts — the whole brief — deciding each stage alone from its own argmax. One turn, one round per stage. Its window holds the entire history, and compacts by eviction. |
| `solo+fold` | The same agent, compacting by a **superseding account** instead: what falls out of the window is summarised rather than dropped, and still counts at reduced fidelity. Compaction done properly. |
| `hive+` | The room, deliberating each stage at the tuned policy. Each member holds only what it was given, so its window carries a fraction of what `solo`'s does. |
| `hive+pooled` | The ceiling: every reading in every member's hands, free. |

### Compaction has two shapes

[`ContextBudget`] gains a mode, because "the window overflowed" and "the agent
compacted" are different events with different costs:

- **`Compaction::Evict`** — today's behaviour and the default. Rows past
  capacity are dropped from the middle. What is gone is gone.
- **`Compaction::Fold`** — a row past capacity is *summarised*: it no longer
  occupies a position, and it still contributes at `FOLD_FIDELITY`. This is a
  deliberately generous model of compaction, because the point of `solo+fold`
  is to make the solo agent as strong as it can honestly be made.

`Compaction::Evict` is the default and every existing arm keeps it, so no
recorded number moves.

[`ContextBudget`]: ../../crates/tinyhivemind-hive/examples/bench/context.rs

### The cost columns

`held/ep` reports the mean rows a participant was carrying when the chain
ended. It is the axis this task exists to stress, and the one that separates a
room from a soloist: a room of *n* splits the same brief *n* ways.

## Acceptance criteria

1. `--stages 1` reproduces the single-shot arms exactly.
2. Stage *k*'s topics are disjoint from stage *j*'s for `j != k`, so no reading
   is ever scored against the wrong stage.
3. A member's context at stage *k* contains every entry it held at stage *k-1*.
4. `end-to-end %` is never above the minimum per-stage rate, and equals
   `stage %` at one stage.
5. `Compaction::Evict` is the default; a run that does not ask for `Fold` is
   bit-identical to one built before it existed.
6. `solo` beats `hive+` at short horizons with a generous window, or the
   experiment records that it does not. **Met:** it does, and the crossover
   arrives at a window of about sixteen rows.
7. The result is recorded in `docs/experiments/` whether or not the room wins.
   **Met, and the room loses:** `solo+fold` beats `hive+` at every horizon and
   every window measured.

## Known weakness

A window never binds for a member of the room, because its pooling happens
through the floor and this harness does not charge a transcript read to
anybody's window — only what a member was told *privately* occupies a row. The
room therefore runs the comparison with a free channel. That favours the room,
and the room loses anyway, so the conclusion is robust in the direction that
matters; charging the read would move `hive+` down and could not move it up.
Making that charge real is the obvious next piece of work and is not
implemented.
