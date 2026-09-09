# Task variety: several facets at once

**Status** Accepted · **Applies to** `crates/tinyhivemind-hive/examples/bench`
· **Follows** [`long-horizon-tasks.md`](long-horizon-tasks.md)

## Why

[The long-horizon experiment](../experiments/2026-09-09-the-long-horizon.md)
asked whether **length** makes a room worth its price and answered no. A
soloist that compacts by a superseding account — `solo+fold` — beat a room of
five at every horizon and every window, and the room paid about seven times the
depth to lose. Length is not the axis.

The claim left standing is about **variety**. A real task is rarely one
question asked sixteen times; it is a rollout decision and a migration decision
and a naming decision, each wanting a different kind of attention, none of them
waiting on the others. One agent has one set of priors and one window. A room
has several of each.

Nothing in the harness could state that. `--stages` grows the task through
time; `--scale-sweep` grows the *room*; neither grows the task **sideways**.
This specifies the regime that does.

## What

`--facets F` runs **F independent sub-decisions belonging to one task**.

Two properties make them a spread rather than a chain:

1. **They do not compound through time.** No facet poisons another — there is
   no analogue of `POISON_LIFT` here and there must not be. What compounds is
   the *conjunction*: `all-facets %` counts a task done only when every facet
   is right, so the headline falls with `F` for every arm and the question the
   sweep answers is how fast.
2. **Each facet names its options distinctly** (`Room::for_facet`, suffix
   `f{n}`), so a reading taken on one facet can never be scored against
   another's question. It can only take up a row — which is exactly the cost a
   participant holding the whole task pays and one holding a single facet does
   not. The suffix is distinct from `--stages`' bare index, so the two regimes
   could never score one against the other.

`--roles` adds the second mechanism: each facet has an **owner**,
`facet % members`, fixed by construction rather than drawn, who reads that
facet at the room's base noise while everybody else reads it at
`LAY_NOISE_PERCENT` of it. Information is redistributed, never created — the
owner's reading is unchanged from a uniform room's, and only the room around it
widens.

It is deliberately weaker than `Expertise::Specialists`, which divides an
expert's noise by `EXPERT_NOISE_DIVISOR` as well. At the noise this harness
runs at, that division puts an expert's error far inside the sixty-point gap
between the true option and a decoy: an owner would answer its own facet
correctly essentially always, and the sweep would measure that constant rather
than the room. A first run confirmed it — `hive+fold` read **100.0** at every
width — which is why the weaker rule is the one specified.

Roles are off by default, so the sweep's own baseline measures the division of
labour and nothing else.

## The arms

| arm | what it is |
| --- | --- |
| `solo` | one agent handed every member's readings on every facet, deciding all of them alone in one window, compacting by eviction |
| `solo+fold` | the same agent compacting by a superseding account — the arm that won the horizon |
| `hive+fold` | **the arm under test.** Facet `f` is decided by member `f % n`, which reads its peers' readings *of its own facet* and nothing of anybody else's, folds what overflows, and carries only the facets it owns |
| `hive+alone` | **its matched control.** The same seat with the pooling removed and nothing else changed |
| `hive+` | the room deliberating every facet on the floor — every member carries every facet's brief |
| `hive+pooled` | the ceiling: every reading in every member's hands, free |

`hive+fold` is the proposal this regime exists to test, stated in the harness's
own units: *keep the thing that won, and make one of them a seat*. Its depth is
`⌈F / n⌉` rounds rather than `F`, because independent facets are precisely what
[ADR 0014](../adr/0014-a-round-authorizes-concurrent-turns.md) authorizes one
round to run at once.

`hive+alone` is not decoration. Splitting a task and *not* sharing what the room
knows about each piece is the obvious failure mode, and without the control the
sweep could not tell a win bought by the split from a win bought by the pooling.

## Acceptance

1. `--facets 1` puts `hive+fold` and `solo+fold` within noise of each other:
   one task, one agent, no room needed. This is a *prediction the regime must
   reproduce*, not a definition.
2. At an unbounded window every pooling arm scores identically at every width.
   A win that appears without a window is a bug in the load accounting.
3. Each facet's option names are disjoint from every other facet's, and from
   every stage's.
4. `all-facets %` never exceeds `facet %`, and equals it at `F = 1`.
5. A seat that owns no facet is left out of the load average rather than
   counted as an idle seat holding nothing.
6. Every published number outside `--facets` is bit-identical, because
   `Expertise::Roles` is a new variant nothing else selects and `for_facet` is
   called by nothing else.
7. The result is recorded whether or not the room wins.

## What follows

The measured shape is `hive+fold`, and it is the shape this repository builds
on from here: **one task, one agent; two or more facets, one seat each.** The
library does not offer it yet — a host gets the floor from
`tinyhivemind-hive` and would have to assemble the split itself. Making it a
first-class, bounded, pure fold is P24 in [`ROADMAP.md`](../../ROADMAP.md), and
the on-floor room stays as the arm it beat rather than being removed.

## Known weaknesses

- **The soloist aggregates by an unweighted mean.** `SimAgent::import` averages
  every reading it takes, so a soloist handed one sharp reading and four wide
  ones averages them flat. A soloist that knew whose reading to trust would
  score higher, and the gap this regime measures would narrow. The room is
  *not* given that knowledge either — a facet's owner reads its own facet, it
  does not weight anybody — so the comparison is between two equally naive
  aggregators, but a smarter soloist is the first thing to try against this
  result.
- **A fold's fidelity does not decay with how much is folded.**
  `Compaction::Fold` is worth a constant `FOLD_FIDELITY` per evicted row
  regardless of how many rows the account is standing in for, which is why
  `solo+fold` is nearly window-immune. Real summarisation degrades with the
  compression ratio. Sweeping `--fidelity` is how much this assumption is
  worth; making it decay would move `solo+fold` and every horizon number with
  it, and is out of scope here.
- **The room's floor reads are still not charged to a window**, carried over
  from the horizon spec. It flatters `hive+`, which loses anyway.
