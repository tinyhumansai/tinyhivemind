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
- [`delegation.md`](delegation.md) — how a collective decides which specialist
  acts: response thresholds, the tremble dance, transactive memory, hidden
  profiles, and the 2023-2026 landscape of agentic routers, with the honest
  half of the evidence against each.
- [`long-context.md`](long-context.md) — position bias in a long window,
  context rot, and recursive language models: why P14 makes the transcript
  queryable instead of making the window bigger.
- [`context-in-agent-teams.md`](context-in-agent-teams.md) — what it costs when
  two agents on one desk hold different transcripts: the Cognition/Anthropic
  disagreement about sharing context, hidden profiles, the conformity that full
  visibility buys, the auditability a private channel owes, and the four ways a
  divergent view fails a reader with a sliding window. The reading behind P17.
- [`multi-context.md`](multi-context.md) — N seats, N windows, one record:
  why no shipping design gives its orchestrator the full context, what a seat
  carries between turns instead (context folding, externalized plans), and the
  feedthrough a shared artifact already emits. The reading behind seat
  continuity and [`../specs/thoughts-and-channels.md`](../specs/thoughts-and-channels.md).
- [`grok-bots/`](grok-bots/README.md) — twelve notes on the open-source Grok
  Bot ecosystem, read at pinned commits: how each one models a roster, what
  makes a message start a turn, and how four of them shipped uncontrolled
  fan-out and then bought a bound.

Each closes with a table mapping the mechanisms it covers to the state this
workspace already holds, and to the state it does not.
