# Depth and width

Split out of [`README.md`](README.md), which is capped at 500 lines. That file
says what the arms are; this one says what the two cost columns mean and what
the concurrency arms scored.

`turns/ep` is what the turn budget bounds. `rounds/ep`, beside it, is what a
host with async seats actually waits for: every turn in one round is authorized
against the same transcript and none of them can read another, so they cost one
wait between them. Until ADR 0014 the two were one number, and the table charged
`vote` fifteen turns for what is **one round** of fifteen independent answers —
so the matched-budget control was never spending more wall clock than the
deliberation, but about seven times less.

Width turns out to be two mechanisms with opposite prices, which is why the
policy carries two bounds. Widening a **blind** round is free, because a blind
member could not read a concurrent peer's row in any case; widening a
**revealed** round is not, because a revealed member's turn depends on exactly
that row.

```text
arm       turns/ep rounds/ep   decided %   correct %
hive+         6.75      6.75        99.4        82.1
hive+wide     7.64      3.38        89.5        77.1
hive+blind    6.75      3.75        99.4        82.1
```

The numbers across the room sizes, and what this does *not* show — concurrency
does not rescue the scale collapse — are in
[`../../../../docs/experiments/2026-09-09-depth-and-width.md`](../../../../docs/experiments/2026-09-09-depth-and-width.md).

