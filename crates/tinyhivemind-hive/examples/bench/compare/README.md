# `compare/`

The simulated multi-arm comparison: every mechanism probe run over the same
rooms from the same private evaluations, at one point of the grid.

This is the table that answers *is this one move worth its turns* — each arm
differs from its control in exactly one thing. The question *how do these
systems behave across problem, size, difficulty and concurrency* is a different
one, and [`../grid/`](../grid/) answers it over a cross product instead.

| file | what it holds |
| --- | --- |
| `mod.rs` | `compare` itself: the arm list, the printed and `--json` tables, the paired-bootstrap differences against the controls, and the endings summary |
| `totals.rs` | `Totals` — every arm's running aggregate, its construction at a chosen cost model, and the fold that merges per-room chunks back in room order |

## Operational constraints

- **Every arm in one run is priced at one cost model.** `Totals::priced_at`
  sets it on each aggregate, `ladder_directed` included — it sits outside
  `arms_mut`'s array and has to be set explicitly, or its row would report
  against different constants from the header printed above it.
- **The merge is order-independent.** Rooms are decided in workers and folded
  in room order, so `--jobs` never moves a printed number.
