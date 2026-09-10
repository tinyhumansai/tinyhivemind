# `division/` — a task's facets, split across the seats that own them

Every other directory here helps a room decide **one** question. This one
decides how many questions there are, who takes each, and what each owner
reads.

| file | what it holds |
| --- | --- |
| `mod.rs` | `divide`, the fold, and the two private halves it is made of: `owner_of` (who takes a facet) and `packed` (which of them ride the same round) |
| `types.rs` | `Division`, `Assignment`, `OwnerReason`, and `DivisionPolicy` |
| `test.rs` | unit tests: the one-facet rule, round packing, the directory path and its control, and every failure path |

## Why it exists

Two experiments, in order.

[The long horizon](../../../../docs/experiments/2026-09-09-the-long-horizon.md)
asked whether a room earns its price on a task that gets *longer*, and answered
**no**: a single agent compacting by a superseding account beat a room of five
at every horizon and every window, at a seventh of the depth.

[Variety and roles](../../../../docs/experiments/2026-09-09-variety-and-roles.md)
asked whether it earns it on a task that gets *wider*, and answered **yes**,
with the crossover at two facets:

```text
all-facets %       F=1      F=2      F=4      F=8
one agent         78.7     56.9     28.1      7.8
one seat each     78.7     61.1     35.9     13.0
```

So this module is the shape the benchmark actually endorses, and it is the
reason `DivisionPolicy::DEFAULT` is **on** where every other opt-in mechanism
here ships off. A default that made a caller opt into the arm that wins would
be the wrong way round.

## The two halves

Both are needed. Dropping either loses the result.

- **Who owns what.** `divide` asks the [`Directory`](../directory/README.md)
  first and falls to a rotation second, recording which it got as
  `OwnerReason`. The distinction is not decoration: the benchmark measured
  that dividing a task across seats buys, on its own, **zero** — 33.9% against
  a soloist's 34.0% — and that dividing it along a line where competence
  differs is the whole effect.
- **What each owner reads.** `Division::scoped` gives an owner its own facet's
  rows and none of the others'. This is the half that wins. A seat's accuracy
  is flat as a task widens (77.9% at one facet and at eight) because it never
  holds the facets it is not deciding; a soloist's decays because it holds all
  of them, at 160 rows against 32.

## What it deliberately does not do

- **It does not pool.** An owner must still see its peers' readings *of its own
  facet* — those rows are in scope, and putting them there is the host's job.
  The control matters: splitting a task and pooling nothing scored **1.7**
  against the division's 13.0, which is worse than not splitting at all.
- **It does not decide what a facet is.** The caller names them. This crate has
  no opinion about how work decomposes, only about what to do once it has.
- **It does not wait.** `depth()` is a *number of rounds*; a host running async
  seats does the waiting. `divide` opens nothing and stores nothing between
  calls, and the same arguments produce the same division every time.

## The bounds

A round runs at most `DivisionPolicy::round_width` assignments and never names
one seat twice, because one agent cannot answer two questions concurrently.
Both bounds are the same argument [ADR 0014](../../../../docs/adr/0014-a-round-authorizes-concurrent-turns.md)
makes about a deliberation's round: what the removed one-turn rule was guarding
was unbounded fan-out, and the bound is the invariant that replaced it.

A task of one facet divides into one assignment in one round — a single agent
answering alone. That is not a special case in the code and must not become
one: it is the measured rule falling out of the general one, and
`Division::is_alone` is how a caller reads it.
