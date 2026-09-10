# `grid/`

The benchmark matrix: one table shape over the cross product of four axes.

Before this existed, the four questions a host actually asks — how does this
behave on my kind of problem, at my size, at my difficulty, at the concurrency
I can afford — were answered by five separate sweep modes, each printing a
table of its own shape. No two of them could be read against each other, so the
axis a finding depended on was never visible in the table reporting it.

Every cell here runs the same arms and reports the same six columns, so a row
and the row beneath it differ in one axis value and the comparison is the
reader's to make rather than the author's to assert.

| file | what it holds |
| --- | --- |
| `mod.rs` | the five systems a cell runs, the per-cell rooms and their seeding, the one table shape, and the across-cells summary that says which arm led on each metric and where |
| `axes.rs` | the four axes and what a point on each means: `Topic` (who knows what), `Complexity` (options on offer and evaluation noise, as one ordinal knob), `Cell`, `Axes`, and their parsing |
| `test.rs` | what the axes parse to, what they refuse, and that a cell's seed does not move when the grid around it does — which is what makes a cell's label a complete reproduction recipe |

## Operational constraints

- **A bare `--grid` is one cell.** A benchmark whose cheapest invocation is a
  sixty-cell run is one nobody runs before pushing, and a benchmark nobody runs
  stops being true. Each axis is widened by naming it.
- **An off-axis value stops the run.** Skipping it would run a smaller grid
  than was asked for and print it under the heading that was typed.
- **Each cell seeds its rooms from its own coordinates**, so two cells are
  different rooms rather than the same rooms relabelled, and re-running one
  cell alone draws exactly the rooms it had inside the whole grid.
- **Cells run in order**, so output streams and a long run can be read before
  it finishes. `--jobs` spreads the rooms *inside* a cell across cores.
