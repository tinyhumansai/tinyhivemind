# The benchmark matrix

Accepted behaviour for how `tinyhivemind-hive`'s benchmark executes, what it
reports, and what it refuses to report. Supersedes nothing — the arms and
sweeps it describes all still run — but it fixes the *shape* the results are
reported in, which was the defect.

## The defect

The harness answered four separate questions with four separate sweep modes,
each printing a table of its own shape, and reported them in units that could
not be compared with each other:

- `correct %` measured the protocol. Real, and the only column that did.
- `ns/step` measured the **library**, with every agent's own time excluded.
- `episodes/s` measured the library too, so `vote` — which never calls the
  library — printed `ns/step 0` and `episodes/s inf`.
- `turns/ep` measured width and `rounds/ep` depth, in units of turns.
- `cost/ep` counted turns again, in abstract units.

Nothing in that list is what a host pays. There was no token count, no latency,
no throughput and no tokens-per-second anywhere in the harness, so the central
trade — a deliberation is slower and cheaper than a poll, by roughly six times
and two times respectively — could not be read off any table it printed. Worse,
the columns that *were* printed invited the opposite conclusion: an arm with a
low `ns/step` looked cheap.

Twenty-four arms in one flat table, most reporting an identical number,
followed by forty-eight lines of paired differences, made the rest.

## What is required

### R1 — Six columns, the same six for every arm

Every arm in every table reports exactly these, and each is a quantity somebody
pays or receives:

| column | definition |
| --- | --- |
| quality | episodes deciding the genuinely best option, as a share |
| speed | mean milliseconds from brief to decision |
| thru | episodes one seat pool finishes per hour at that latency |
| conc | turns taken divided by rounds taken |
| tok/ep | prompt plus completion tokens, per episode |
| tok/s | those tokens divided by wall-clock seconds |

### R2 — Wall clock is a sum over rounds; tokens are a sum over turns

Turns inside one round are authorized against the same transcript and cannot
read each other ([ADR 0014](../adr/0014-a-round-authorizes-concurrent-turns.md)),
so a host with a seat free for each waits once for the whole round however wide
it is. A round's wall clock does not grow with its width; its tokens do.

This asymmetry is what makes `conc` a column worth printing rather than a
restatement of `turns/ep`.

### R3 — The cost model is visible, movable, and measurable

Five of the six columns are computed from five constants, four of which
`--calibrate` fits from the endpoint. The fifth, `tokens_per_turn`, is a
property of the prompt this harness sends rather than of the endpoint, so
calibration reports what its probes wrote rather than fitting it. Therefore:

- every run prints them, with the flags that reproduce them, above every table;
- each has a flag;
- `--calibrate` fits the four endpoint constants, as two two-point fits —
  prompt tokens against transcript rows, and latency against completion length.
  A one-point probe cannot separate either pair;
- a constant that will not parse **stops the run**. Defaulting it to zero would
  report free tokens or an instant answer under the heading the operator typed,
  and would swallow the next flag as its argument.

### R4 — The round shape is recorded, not reconstructed

An episode records `(rows visible, turns authorized)` per round as it runs.
`(turns, rounds)` cannot distinguish `4,1,1,1` reading a growing transcript
from `1,1,1,4` reading a short one, and those price differently. Off-floor
exchange rounds are recorded too, since a host waits for those as well.

### R5 — Four axes, one cross product, one table shape

`--topic`, `--scale`, `--complexity`, `--concurrency`. Every cell runs the same
arms and reports the same six columns, so adjacent rows differ in exactly one
axis value.

- A bare `--grid` is **one cell**, reproducing the single-room comparison. A
  benchmark whose cheapest invocation is a sixty-cell run is one nobody runs
  before pushing, and a benchmark nobody runs stops being true.
- Naming any axis selects the grid.
- A value that is not a point on its axis **stops the run**. Skipping it would
  run a smaller grid than was asked for under the heading that was typed.
- Each cell seeds its rooms from its own coordinates, so re-running one cell
  alone draws exactly the rooms it had inside the whole grid. A cell's label is
  a complete reproduction recipe.

### R6 — The library's own cost keeps a heading of its own

`ns/step`, `turns/ep`, `rounds/ep` and `decided %` are still printed, under a
heading saying they exclude every agent's own time. They may not share a table
with token or latency columns, because `vote`'s `ns/step 0` there reads as
"this arm is free" rather than "this arm never calls the library".

### R7 — The grid runs systems; the flat table runs mechanisms

The grid runs five systems a host chooses between. The twenty-four-arm table is
a set of *mechanism probes*, each differing from its control in exactly one
thing, and it runs at one point. Reproducing twenty-four arms in every cell
would print hundreds of rows to answer a question about four axes.

## What is deliberately not required

**Queueing.** Rounds are priced as though a seat were free for every turn
authorized, so `conc` is the width the protocol asked for rather than the width
a deployment could afford. Charging for a seat pool would make arms
incomparable across the scale axis, which is the axis the grid exists to walk.

**Prompt caching.** The model charges full prompt price on every turn, so it
overstates the token cost of deep arms relative to wide ones. Measuring it
needs `--api-base`.

**A claim that any of this transfers to language models.** The participants are
arithmetic. What the numbers show is which *policy* aggregates information and
which throws it away, which is the question a host answers when it configures a
room — not that language models deliberate well.

## Verification

- `cost/test.rs` — a wider round costs more tokens and no more wall clock; a
  sample's cost folds identically however the threads split it (the property
  `--jobs` depends on); an unparsable constant leaves the model untouched.
- `grid/test.rs` — the default is one cell; the cross product is every
  combination; adjacent cells differ in one axis; a cell's seed does not move
  when the grid around it does; every off-axis value is refused.
- `calibrate/test.rs` — the fit arithmetic, and that each probe pair differs in
  exactly one thing.
