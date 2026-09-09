# Variety, not length: what a room is actually for

**Date** 2026-09-09 · **Branch** `concurrent-rounds` ·
**Spec** [`docs/specs/task-variety.md`](../specs/task-variety.md)

**Code**
`cargo run --release -p tinyhivemind-hive --example bench -- --facets 1,2,4,8 --episodes 2000 --rot 0.5 [--roles] --context {0,32,16,8}`

## The question

[The long-horizon run](2026-09-09-the-long-horizon.md) killed the length
hypothesis. A soloist compacting by a superseding account beat a room of five
at every horizon and every window and paid a seventh of the depth. If a room
earns its price anywhere it is not by working *longer*.

The claim left standing is that it earns it by working *wider*: a task that is
several questions at once, each wanting different attention, none waiting on
the others. So this run keeps the winner and changes its shape — **each
`solo+fold` becomes a seat**, one facet each.

## The result

`all-facets %` — every one of `F` facets right — with each facet owned
(`--roles`), 2000 tasks per width, five members, rot 0.5:

```text
window 8            F=1      F=2      F=4      F=8
solo               63.3     35.9     12.2      1.5
solo+fold          78.7     56.9     28.1      7.8
hive+fold          78.7     61.1     35.9     13.0
hive+alone         56.6     32.5     11.2      1.7
hive+              68.2     46.8     21.1      4.1
hive+pooled        64.8     42.7     18.1      3.4
```

**The prediction held, on both halves.**

At **one facet** `hive+fold` and `solo+fold` are *identical* — 78.7 against
78.7, the same seed, the same arithmetic, no room needed. From **two facets**
the room pulls ahead and the gap widens with width: +4.2, +7.8, **+5.2 points
at F=8, which is 1.7× the soloist's score**.

The mechanism is legible in the per-facet column:

```text
facet %             F=1      F=2      F=4      F=8
solo+fold          78.7     75.3     73.0     73.2
hive+fold          78.7     77.8     77.9     77.9
```

`hive+fold` is **flat in `F`**. The soloist decays because its window fills with
facets it is not currently deciding; a seat's does not, because it never holds
them. Load says the same thing, and the depth column says what it cost:

```text
held/ep             F=1      F=2      F=4      F=8      rounds/ep at F=8
solo+fold          20.0     40.0     80.0    160.0                   8.0
hive+fold          20.0     20.0     20.0     32.0                   2.0
hive+               4.0      8.0     16.0     32.0                  58.4
```

Five times the accuracy-per-row and a quarter of the depth, because independent
facets ride one round — which is what the concurrency landed in
[ADR 0014](../adr/0014-a-round-authorizes-concurrent-turns.md) is for.

## And the room only wins when the window binds

At an unbounded window every pooling arm is identical at every width:

```text
window unbounded    F=1      F=2      F=4      F=8
solo+fold          77.7     61.0     36.5     13.3
hive+fold          77.7     61.0     36.5     13.3
```

There is no free lunch in the split. The advantage is *entirely* the window,
and it appears in order as the window tightens — nothing at 32 rows worth
naming, +2.7 at 16, +5.2 at 8.

## The mechanism is roles, not the split

Run the same sweep without `--roles`, where every member is equally competent
and the arms differ only in how the load is divided:

```text
window 8, shared    F=1      F=2      F=4      F=8
solo+fold          87.6     76.5     59.0     34.0
hive+fold          87.6     76.0     59.8     33.9
```

**Nothing.** Dividing a task across seats buys, on its own, zero — at F=2 it
even loses. What buys is dividing it **along a line where competence differs**:
the owner's good reading sits at the edge of a small window, undiscounted,
where in the soloist's crowded window the same reading is buried in the middle
and the U-curve discounts it. Splitting the load is not the point. Splitting it
where the priors differ is.

## The control earns its place

`hive+alone` — the same seat with its peers' readings taken away — scores
**1.7** where `hive+fold` scores 13.0. Splitting a task and *not* pooling what
the room knows about each piece is worse than not splitting it at all. The win
is the split **and** the pooling, and without the control this run could not
have said which.

## What would have to be true for this to be wrong

The result rests on `Compaction::Fold` being worth a constant per evicted row,
which is generous to the soloist and is the assumption
[the spec](../specs/task-variety.md) flags. Sweeping it, at F=8, window 8,
roles:

```text
--fidelity         0.35      0.2      0.1      0.0
solo+fold           7.8      4.3      2.4      1.5
hive+fold          13.0      9.2      5.8      2.8
```

The ordering survives the whole range, including `0.0`, which is compaction by
pure eviction. The finding does not depend on the one number it could have.

What it *does* still rest on: the soloist aggregates by an unweighted mean, so
it averages one sharp reading with four wide ones. A soloist that knew whose
reading to trust would narrow this gap, and that is the first thing to try
against this result. The room is given no such knowledge either — an owner
reads its own facet, it does not weight anybody — so the comparison is between
two equally naive aggregators.

## Reading

A room is not a longer agent. It is a **wider** one, and only where the work has
edges to divide it along:

- **One task: keep `solo+fold`.** Identical to the room, cheaper, no protocol.
- **Two or more facets with different owners: split them.** Flat accuracy in
  width, and at F=8 a fifth of the rows and a quarter of the depth -- the ratio
  itself narrows at a lower width (one half at F=2, one quarter at F=4).
- **The floor is not the mechanism.** `hive+` — the room deliberating every
  facet together — scores 4.1 against `hive+fold`'s 13.0 and spends **29×** the
  depth. Every member carrying every facet's brief is the soloist's problem
  with extra steps.
