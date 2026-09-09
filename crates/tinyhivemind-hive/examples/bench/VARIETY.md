# A task with several facets at once

Split out of [`README.md`](README.md), which is capped at 500 lines. The full
argument and every caveat are in
[the experiment](../../../../docs/experiments/2026-09-09-variety-and-roles.md)
and [the spec](../../../../docs/specs/task-variety.md); this is the file-level
reference for what `--facets` builds.

## The spread

`--facets F` runs F **independent** sub-decisions belonging to one task. Two
things make it a spread rather than a chain:

- **Nothing compounds through time.** No facet poisons another. What compounds
  is the conjunction: `all-facets %` counts a task done only when every facet
  is right.
- **Each facet names its options distinctly** (suffix `f{n}`, disjoint from
  `--stages`' bare index), so a reading taken on one facet can only ever occupy
  a row in another's window — never be scored against its question.

`--roles` gives each facet an owner, `facet % members`, fixed by construction.
The owner reads that facet at the room's base noise and everybody else at
`LAY_NOISE_PERCENT` of it. Information is redistributed, never created:
`Expertise::Roles` is deliberately weaker than `Expertise::Specialists`, which
would divide the owner's noise by `EXPERT_NOISE_DIVISOR` as well and make an
owner effectively infallible — the first run of this sweep read **100.0** at
every width, which measures the constant and not the room.

## The arms

| arm | what it is |
| --- | --- |
| `solo` | one agent handed every member's readings on every facet, deciding all of them alone in one window, compacting by **eviction** |
| `solo+fold` | the same agent compacting by a **superseding account** — the arm that won the horizon |
| `hive+fold` | facet `f` decided by member `f % n`, which reads its peers' readings **of its own facet only**, folds what overflows, and carries only the facets it owns. Depth is `⌈F / n⌉` rounds, not `F` -- priced from the library's own seat assignment (`tinyhivemind_hive::division`), the rounds a host running the seats concurrently would pay, not a wall-clock observation: the harness itself still evaluates each facet in a sequential loop |
| `hive+alone` | the matched control: that same seat with the pooling removed and nothing else changed |
| `hive+` | the room deliberating every facet on the floor — every member carries every facet's brief |
| `hive+pooled` | the ceiling |

## What it found

`all-facets %`, five members, `--roles`, rot 0.5, 2000 tasks per width:

```text
window 8            F=1      F=2      F=4      F=8
solo               63.3     35.9     12.2      1.5
solo+fold          78.7     56.9     28.1      7.8
hive+fold          78.7     61.1     35.9     13.0
hive+alone         56.6     32.5     11.2      1.7
hive+              68.2     46.8     21.1      4.1
```

At **one facet** the room and the soloist are identical — keep `solo+fold`. From
**two** the room pulls ahead, and its per-facet rate is *flat* in width (77.9)
where the soloist's decays (78.7 → 73.2), because a seat never holds the facets
it is not deciding: 32 rows against 160, and 2 rounds against 8.

Three things bound the claim:

- **At an unbounded window every pooling arm is identical at every width.** The
  advantage is entirely the window; there is no free lunch in the split.
- **Without `--roles` the effect vanishes** (33.9 vs 34.0 at F=8). Dividing the
  load buys nothing; dividing it along a line where competence differs does.
- **`hive+alone` scores 1.7.** Splitting a task and *not* pooling what the room
  knows about each piece is worse than not splitting it at all.
