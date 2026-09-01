# Research

Where the mechanisms in this workspace come from, at more depth than
[`../../wiki/Further-reading.md`](../../wiki/Further-reading.md) carries.

The wiki page is a reader's map: one paragraph per mechanism, linked to the
page that uses it. These files are the working notes behind it — the equations,
the measured constants, the exact citation, and, for each mechanism, the one
line that matters here: **what a shared transcript would have to represent to
implement it**.

A note is worth writing when a design decision needs a warrant that is longer
than a comment and is not itself a decision. Decisions go in
[`../adr/`](../adr); behavior goes in [`../specs/`](../specs/README.md);
what actually happened when it was run goes in
[`../experiments/`](../experiments). These files hold only the reading.

## Notes

- [`biology.md`](biology.md) — stigmergy, quorum sensing, honeybee nest-site
  selection and its differential equations, response thresholds, and the limits
  of collective intelligence.
- [`shared-context.md`](shared-context.md) — the human and organizational half
  (transactive memory, distributed cognition, grounding, boundary objects,
  awareness) and the open-source landscape of shared agent memory.
- [`long-context.md`](long-context.md) — position bias in a long window,
  context rot, and recursive language models: why P14 makes the transcript
  queryable instead of making the window bigger.

Each closes with a table mapping the mechanisms it covers to the state this
workspace already holds, and to the state it does not.
