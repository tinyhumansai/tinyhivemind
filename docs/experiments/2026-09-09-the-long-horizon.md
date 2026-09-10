# Does a room beat a single agent on a long task?

**Date** 2026-09-09
**Status** Recorded
**Code** `cargo run --release -p tinyhivemind-hive --example bench -- --stages 1,2,4,8,16 --episodes 1000 --rot 0.5`, swept over `--context 0,32,16,8`, on `concurrent-rounds`.
**Spec** [`../specs/long-horizon-tasks.md`](../specs/long-horizon-tasks.md)
**Sample** 1000 seeded chains per horizon, every arm deciding the same chains.
Simulated participants, so the numbers reproduce from the seed.

## The question

The claim this library exists to test, stated plainly:

> A single agent with context compaction wins at small scale and short horizon.
> A room of agents should win as the work gets longer.

Until `--stages` neither half was stateable. The benchmark measured **one**
categorical choice, so nothing compounded and no window ever bound; and the
control the claim is about — *one agent working a long task, compacting as it
goes* — was not an arm. `ladder` is one turn and `vote` is `n` one-shots.

`--stages N` runs N sub-decisions in sequence. Stage `k`'s options are named
distinctly, so a reading taken at one stage can only take up a row at another;
what a member was told persists in its window; and a stage decided wrongly lifts
the next stage's decoy by `POISON_LIFT`, because the room is now reasoning from
a false premise. `solo` is the room's own member handed **every** peer's
readings and facts — the whole brief in one window.

## Compounding is real, and it hits everybody

Unbounded window, so this is the task and nothing else:

```text
end-to-end %         1        2        4        8       16
solo              91.5     84.5     71.3     51.6     26.1
hive+             82.5     68.1     45.6     22.7      5.1
```

Per stage, both arms are almost flat — `solo` 91.5 → 90.1, `hive+` 82.5 → 78.8.
End-to-end falls off a cliff anyway, because sixteen stages at 90% is 19%. That
is the whole difficulty of long work and it is not a context problem.

With no window pressure the soloist simply wins: it is better informed, holding
every brief the room holds between them.

## The crossover exists

Squeeze the window and the ordering changes.

```text
end-to-end %, 16 stages    solo   solo+fold   hive+
window unbounded           26.1        26.1     5.1
window 32                  24.1        24.1     5.1
window 16                   4.8        17.5     5.1
window 8                    0.5        13.1     5.1
```

At a window of eight, `hive+` beats `solo` at **every** horizon — 82.5 against
73.0 at one stage, 5.1 against 0.5 at sixteen. The mechanism is exactly the one
predicted: the soloist carries the whole brief and the room splits it.

```text
held/ep — rows one participant carries
arm                  1        2        4        8       16
solo              20.0     40.0     80.0    160.0    320.0
hive+              4.0      8.0     16.0     32.0     64.0
```

Five members, five times fewer rows each. **So the thesis is confirmed against
an agent that compacts by dropping rows.**

## And it evaporates against compaction done properly

`solo+fold` is the same agent, summarising what leaves the window instead of
discarding it — a row past capacity gives up its position and still counts at
`FOLD_FIDELITY`.

```text
end-to-end %, window 8     1        2        4        8       16
solo                    73.0     53.9     26.1      7.8      0.5
solo+fold               87.5     77.1     59.0     36.6     13.1
hive+                   82.5     68.1     45.6     22.7      5.1
```

`solo+fold` beats the room at **every horizon and every window tested**. At
sixteen stages and a window of eight it is 13.1 against 5.1 — more than twice
the room — while spending sixteen rounds against the room's 107.9.

Capacity barely moves it (17.5 at sixteen rows, 13.1 at eight, 14.7 at four).
What hurts a compacting agent is the **U-curve, not the ceiling**: a summarised
row keeps no position and so takes no positional discount, which is why a
smaller window is sometimes worth more than a middling one.

## The room is being flattered, and still loses

The most important line in the table is the one that does not move:

```text
hive+  82.5  68.1  45.6  22.7  5.1     — identical at every window tested
```

A window never binds for a member of the room, because in this harness the
room's pooling happens **through the floor** — supporters counted off the
transcript — and the transcript is not charged to anybody's window. Only what a
member was *told privately* occupies rows.

So the room runs this experiment with a free channel, and `solo+fold` beats it
anyway. Charging the room's reads would move `hive+` down and could not move it
up, so the conclusion is robust in the direction that matters. Making that
charge real is the obvious next piece of work and it is **not implemented**.

## What this says

**The thesis is half right, and the half that fails is the half that mattered.**

- *Scale beats a single agent under context pressure* — **yes**, against a
  soloist that compacts by eviction, at a window of sixteen rows or less.
- *Scale beats a single agent on long work* — **no**. Against a soloist that
  compacts by summarising, the room loses at every horizon and every window
  measured, and pays 6.7× the depth to do it.

The binding constraint on this task is not context, exactly as
[`../../crates/tinyhivemind-hive/examples/bench/CONTEXT.md`](../../crates/tinyhivemind-hive/examples/bench/CONTEXT.md)
concluded from the single-shot regime. It is that a room of five pooling through
a floor is worse informed than one agent holding five briefs, and a horizon does
not change that — it multiplies it, through compounding, in whichever direction
the per-stage rate already pointed.

## What this does **not** show

- **A room's reads are free here.** Stated above; it is the largest known
  weakness in the comparison and it favours the room.
- **`FOLD_FIDELITY` is not measured.** `0.35` is a parameter, swept by
  `--fidelity`, not a property of any real summariser. A finding that only held
  at one setting would be a finding about `context.rs`; the ordering above holds
  across the whole window ladder, which is what makes it reportable.
- **One task family, simulated members.** The participants are arithmetic. What
  is measured is whether a policy moves information, not whether a model would
  write a useful row.
- **Nothing about wall clock.** Rounds are counted, not timed.
- **No live run.** No model, no desk.
