# What a turn costs

Five of the benchmark's six headline columns — speed, throughput, concurrency,
tokens per episode and tokens per second — are computed from a **model** of
what one turn costs. This file is that model: what it assumes, how to move it,
how to measure it, and what it refuses to claim.

## Why there is a model at all

Everything the harness counted before this existed was in units only the
harness used: a turn, a round, a `cost_unit`, a nanosecond spent inside `step`.
Those are the right units for asking what the *library* costs, and the answer
is a good one — a few microseconds per step, which is nothing.

They are the wrong units for asking what a *system* costs, and the two were
printed in one table, so they were read as though they were the same question.
The clearest symptom: `vote` reported `0` ns/step and an infinite `episodes/s`,
because a poll never calls the library at all. Read as a system number that
says a poll is free and instantaneous. It is neither — it is the most
token-expensive arm in the table, spending a full prompt for every member of
the room.

Nobody deploying a room cares that the state machine is cheap. They care what
the seats cost and how long the answer takes. So the harness prices the shape
an episode actually ran.

## The model

Four constants stand between a round shape and a millisecond:

| constant | flag | default | what it is |
| --- | --- | --- | --- |
| `tokens_per_row` | `--tokens-per-row` | 42 | prompt tokens one transcript row contributes to a turn that reads it |
| `tokens_per_turn` | `--tokens-per-turn` | 180 | completion tokens one turn writes |
| `prompt_base` | `--prompt-base` | 600 | prompt tokens charged before the first row: system prompt, desk description, task brief, protocol instructions |
| `ttft_ms` | `--ttft` | 450 | milliseconds before a seat's first completion token |
| `decode_tok_s` | `--decode-rate` | 85 | completion tokens a seat decodes per second |

From those, one turn reading `rows` of transcript costs

```text
prompt     = prompt_base + rows * tokens_per_row
completion = tokens_per_turn
latency    = ttft_ms + tokens_per_turn * 1000 / decode_tok_s
```

and an episode is the sum over its rounds — **tokens across every turn, wall
clock across every round**. That asymmetry is the whole reason depth and width
are counted separately: turns inside one round are authorized against the same
transcript and cannot read each other, so a host with a seat free for each of
them waits once for the whole round however wide it is.

It is also what makes `conc` a column worth printing rather than a restatement
of `turns/ep`. An arm that takes seven turns over seven rounds and an arm that
takes seven turns over three rounds cost the same tokens and very different
wall clock.

### `prompt_base` matters more than it looks

A one-turn arm pays it once; a deliberation pays it on *every* turn. An arm's
token cost is therefore not proportional to its turns, which is exactly the
effect the old `cost/ep` column — which counted turns — could never show.

## What it does not model

**Nothing outside a turn.** Every model call a protocol makes is priced, the
responder ladder's router call included — that one is a round of its own,
because nobody can answer until the router has said who answers. What is *not*
priced is anything a host does that is not a model call: retries, tool calls a
seat makes on its own, and the host's own bookkeeping.

**Queueing.** Every round is priced as though the host had a seat free for
every turn the round authorized. `conc` is the width the *protocol* asked for,
not the width a particular deployment could afford. A host with fewer seats
than `round_width` pays more wall clock than this reports, in proportion to how
far short it falls.

That is a deliberate omission rather than an oversight: charging for a seat
pool would make the arms incomparable across the scale axis, which is the axis
the grid exists to walk. A room of 30 against a pool of 8 and a room of 5
against the same pool are not measuring the same thing.

**Prefill separately from time-to-first-token.** Latency does not depend on how
much prompt the turn read. Prefill is overlapped with the TTFT every hosted
model already charges, and a fifth constant for it is one that no calibration
run can separate from the fourth.

**Cache hits.** A deliberation re-sends a growing transcript on every turn,
which is the shape prompt caching is built for, and a provider that caches it
charges a fraction of `prompt_base + rows * tokens_per_row` after the first
turn. The model charges full price every time, so **it overstates the token
cost of the deep arms relative to the wide ones**. If that distinction matters
to a decision you are making, measure it with `--api-base` rather than reading
it here.

## Calibrating it

```sh
cargo run --release -p tinyhivemind-hive --example bench -- \
  --calibrate --api-base "$URL" --model flash
```

Four constants, fitted from real requests, printed as the flags that would pin
a run to them. Each pair is separated by a **two-point fit** rather than an
assumption:

- **`prompt_base` and `tokens_per_row`** — the same request is sent with a
  2-row transcript and a 40-row one. Prompt tokens rise linearly in rows, so
  the slope between the two is the per-row cost and the intercept is everything
  charged before the first row.
- **`ttft` and `decode_rate`** — the endpoint is asked for a 16-token answer
  and a 256-token one. Latency is `ttft + completion / rate`, so again the
  slope is one constant and the intercept the other.

A one-point probe cannot separate either pair. An earlier shape of this that
divided total latency by total tokens reported a "decode rate" that was mostly
time-to-first-token — which is the kind of number that then gets quoted.

Each probe runs three times and the **fastest** is kept. Latency on a hosted
endpoint is noisy enough that one sample of each length can invert the slope
outright and fit a negative decode rate; and the fastest observation is the one
least polluted by queueing at the provider, which is not a property of the
model and is not what these constants are meant to carry.

`tokens_per_turn` is reported but is **not** a property of the endpoint — it is
a property of the prompt this harness sends and the protocol it asks for. Take
it from a real `--api-base` run.

## Reading a result honestly

Every run prints the point it was taken at, with the flags that reproduce it,
above every table. That is the contract: the model is a guess, and a guess is
fine as long as it is visible and movable.

**A finding that survives only at one setting of these constants is a finding
about this file.** Before believing an ordering that turns on speed or tokens,
move the constant it turns on and see whether the ordering holds.

It is worth knowing which constants *can* move an ordering, because two of them
cannot. Every turn in this model is the same length, so `--ttft` and
`--decode-rate` scale every arm's per-round latency by the same factor: they
change how many seconds are on the **speed** and **thru** columns, and they
cannot reorder the arms on them. What they do change is the *trade* — how many
tokens a point of quality costs — because tokens do not scale with them.

The pair that can genuinely reorder is `--tokens-per-row` against
`--prompt-base`. Arms differ sharply in how much transcript their turns read: a
`ladder` turn reads one row, and the last turn of a long deliberation reads
everything before it. Raise the per-row cost and the deep arms get relatively
more expensive; raise the base and the *many-turn* arms do, deep and wide
alike. An ordering on **tok/ep** or **tok/s** that flips between those two
settings is a claim about this file rather than about the protocols, and the
honest report is to say so.

Refusals are deliberate. A cost constant that will not parse stops the run
rather than defaulting to zero, because free tokens or an instant answer,
printed under the heading the operator typed, is worse than no answer at all —
and because a swallowed value would eat the next flag along with it.
