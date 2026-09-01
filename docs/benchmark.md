# Benchmarking bounded deliberation

What a room of agents buys over one agent answering alone, what it costs, and
which policy settings decide the difference. Everything here is produced by
`cargo run --release -p tinyhivemind-hive --example bench`; the harness itself is
described in
[`crates/tinyhivemind-hive/examples/bench/README.md`](../crates/tinyhivemind-hive/examples/bench/README.md).

## Summary

Five agents, four options, 5000 seeded rooms, one core:

```text
arm       turns/ep   decided %   correct %       ns/step    episodes/s
ladder        1.00       100.0        57.6          1109        901660
vote         15.00       100.0        78.5             0           inf
hive          6.16        89.7        73.3          2231         62637
hive+         6.75        99.4        82.1          2278         56641
```

- A tuned deliberation is right **82.1%** of the time in **6.75 turns**.
- The matched-budget control, given all fifteen turns, reaches **78.5%**.
- One responder off the ladder — today's behaviour — reaches **57.6%**.
- The state machine costs about **2.3 µs** per step, six orders of magnitude
  below a model turn.

The margin over the control is a few points, and it is meant to be. Independent
sampling plus a plurality is most of what a room is for, and a protocol that
could not clear that bar would not be worth its budget.

## What is measured

A room chooses between several options, exactly one of which is genuinely best.
Every member holds a *private, noisy* evaluation of every option — the true
quality plus a uniform error of half-width `--noise` — so no member is
individually reliable and the room's only route to the right answer is to pool
what its members separately believe.

The participants are arithmetic, deliberately. A language model would make the
numbers unreproducible and would confound protocol quality with model quality:
what is being measured here is whether a *policy* aggregates information or
throws it away, which is the question a host has to answer when it configures a
desk. Real models appear further down, where the claim is only that they can
hold the protocol.

Everything is seeded. The same `--seed` produces the same rooms, the same
private evaluations, and the same transcripts.

## The arms

Every arm decides the same rooms from the same private evaluations.

| arm | what it is | turns |
| --- | --- | --- |
| `ladder` | `responder_plan` in `tinyhivemind` selects one responder off the real ladder — selector rung included, validated through `accept_selection` — and that agent answers alone. | 1 |
| `vote` | Independent answers decided by plurality, nobody seeing anybody: self-consistency at a matched budget. | the whole budget |
| `hive` | A deliberation episode at `EpisodePolicy::DEFAULT`. | up to the budget |
| `hive+` | The same, at the tuned policy. | up to the budget |

`vote` is the honest control. A multi-agent result without one is close to
meaningless, because the multi-agent arm has usually just spent more compute.
It is given the *whole* budget, which is more turns than the deliberation
actually spends — though with deterministic participants it saturates at one
distinct answer per member, which is exactly what self-consistency does with a
deterministic sampler.

Correctness is scored over the whole sample, including episodes that decided
nothing: an arm cannot buy accuracy by declining to answer.

## Results

### The two bounds on quorum

The quorum threshold is the single most consequential setting, and it has a
bound on each side. Five members, 5000 rooms, everything else held at the tuned
policy:

| quorum | deadlocked | exhausted | decided % | correct % |
| --- | --- | --- | --- | --- |
| 2 of 5 (below a majority) | 514 | 0 | 89.7 | 73.3 |
| **3 of 5 (smallest majority)** | **0** | **29** | **99.4** | **82.1** |
| 5 of 5 (unanimity) | 0 | 2092 | 58.2 | 55.2 |

**Below a majority, rooms deadlock.** Five members can put two grounded
supporters behind each of two options, and an episode in which two options both
carry is deadlocked by definition — no amount of further support resolves it,
because both stay above the line. Requiring a majority makes that state
unreachable and the deadlock rate falls to zero.

**At unanimity, rooms cannot finish.** Cross-inhibition removes a silenced
advocate from a topic's supporter set and does not put them back, so a single
grounded `!object` makes quorum unreachable for the rest of the episode. Two in
five episodes then spend their whole budget without deciding. A live
three-member room hit exactly this, described below.

So: **a majority of the desk, and never the whole of it.** The benchmark's
tuned policy computes `threshold = min(agents / 2 + 1, agents - 1)`.

### The budget has to scale with the desk

A fixed budget makes a larger room look worse than a smaller one, and the
effect is entirely an artifact of the cap. An eight-member room, 1500 rooms
each:

| budget | decided % | correct % | turns actually spent |
| --- | --- | --- | --- |
| 12 | 65.3 | 63.1 | 10.36 |
| 16 | 89.6 | 82.9 | 10.96 |
| 20 | 94.5 | 86.3 | 11.24 |
| 24 | 96.4 | 87.7 | 11.42 |

A blind opening round costs one turn per member before anybody has seen
anybody, a majority then has to assemble on one option, and the decision has to
be recorded. Three turns per member covers that, and it is a *cap* rather than
a cost — the eight-member room finishes in 11.4 turns of the 24 it is allowed.
At five members, budgets of 15, 20 and 25 score 82.0, 82.1 and 82.1: past the
point where the room can finish, extra budget buys nothing.

### The blind round is not decoration

Turning it off, five members, 5000 rooms:

| opening round | decided % | correct % |
| --- | --- | --- |
| blind | 99.4 | 82.1 |
| full visibility | 100.0 | 58.0 |

With full visibility from the first turn the room cascades onto whatever was
proposed first and lands level with a single agent. That is an information
cascade, and `Visibility::Blind` is what prevents it — bought as a filter on
the projection rather than as concurrency, which is the argument of
[`adr/0002-hive-episodes-are-sequential.md`](adr/0002-hive-episodes-are-sequential.md).

### Across desk sizes

2000 rooms per size, threshold and budget scaled as above:

| agents | quorum | budget | ladder % | vote % | hive+ % | turns/ep | decided % |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 3 | 2 | 9 | 57.3 | 68.4 | 71.6 | 4.42 | 99.8 |
| 4 | 3 | 12 | 57.3 | 74.2 | 76.8 | 6.68 | 94.6 |
| 5 | 3 | 15 | 57.6 | 78.8 | 81.5 | 6.79 | 99.3 |
| 6 | 4 | 18 | 57.1 | 82.2 | 83.4 | 9.04 | 95.6 |
| 8 | 5 | 24 | 58.4 | 87.3 | 88.6 | 11.32 | 96.9 |

The deliberation beats the matched-budget control at every size, by 1.2 to 3.2
points, while spending roughly half the turns. Deadlocks are zero throughout.

## What the library costs

`ns/step` is one call to `tinyhivemind_hive::step` over a live transcript, with
the participants' own time excluded: about **2.3 µs**, or roughly 57,000 whole
episodes per second on one core. An episode of nine steps costs about 20 µs of
library time. A model turn is six orders of magnitude more expensive, so the
protocol is free in any real deployment.

Three changes made during this work cut that cost by about a fifth, measured
before and after under identical settings (2816 → 2222 ns/step). None of them
changes behaviour, and every arm's outcome was byte-identical across them:

- `episode::step` folds a borrowed `Vec<&SessionMessage>` rather than cloning
  the filtered transcript on every step, and computes `consensus` once instead
  of twice;
- `trace::extract` returns early on a body containing no `!`, which is most of
  a real transcript, before scanning for fences;
- `quorum::standings` folds on borrowed topic and agent keys and allocates
  owned strings only for what survives, and `attention::bids` hoists saturation
  and the reader-independent half of salience out of its per-member loop.

The remaining cost is dominated by re-reading the transcript on every step,
which is inherent: an episode is a pure fold with no state cached between
calls, so a step over a transcript of *n* messages parses *n* messages.

## Live agents

`--agent-cmd` swaps the simulated participants for a real agent CLI, one
process per authorized turn. These runs used [opencode](https://opencode.ai)
1.18.25 against OpenRouter:

```sh
cargo run --release -p tinyhivemind-hive --example bench -- \
  --agent-cmd "opencode run --pure -m openrouter/~openai/gpt-mini-latest" --agents 5
```

They assert nothing — the deterministic arms measure the protocol, and a
handful of live episodes could not measure anything. What they establish is
that real models hold the trace grammar, and what they surfaced is a set of
host-side obligations the simulation cannot see. All four were fixed in the
harness, and each fix is one a *host* owes its agents rather than something the
library can impose:

| what happened live | the fix |
| --- | --- |
| Models coined `#rollout` and `#rollout-strategy` for one idea; support split across two names never adds up to a quorum. | The prompt names the options already on the floor, folded through the library's own `standings`, with how many supporters each holds and how many it needs. |
| Four consecutive turns restated the same `!question` verbatim. `repetition_cap` damps a restated *support* and cannot see this. | The prompt shows a participant its own last line and asks for something that moves the room on. |
| Models wrote `!commit` while the room was still deliberating, which adds no supporter; one episode spent its whole budget recording a decision it never reached. | The prompt offers only the moves that count in the turn's phase — `!commit` appears only under `Phase::Commit`. |
| Four of five proposals dropped the `#`, so the lines named no topic and deposited nothing. | The grammar's sigils are called out explicitly, with a right and a wrong example. |

A three-member room also exhausted its budget before the policy fix above,
because a threshold of three on a three-member desk is unanimity and one
`!object` had silenced an advocate. That is the second bound on quorum, found
live before it was measured.

After the fixes, a five-member room converged on `#rollout` in 6 turns and 18
seconds of wall clock, with every proposal well-formed and one commit. A
three-member room converged in 5 turns and 14 seconds.

## Reproducing

```sh
cargo run --release -p tinyhivemind-hive --example bench                      # the table above
cargo run --release -p tinyhivemind-hive --example bench -- --episodes 5000
cargo run --release -p tinyhivemind-hive --example bench -- --agents 8
cargo run --release -p tinyhivemind-hive --example bench -- --quorum 5        # unanimity
cargo run --release -p tinyhivemind-hive --example bench -- --no-blind        # the cascade
cargo run --release -p tinyhivemind-hive --example bench -- --sweep           # the policy grid
cargo run --release -p tinyhivemind-hive --example bench -- --trace           # one episode
```

CI runs `cargo run -p tinyhivemind-hive --example bench -- --episodes 25`, so the
harness cannot rot. Live mode is not in CI and needs a configured agent CLI.

## What this does not show

- **Nothing about model quality.** The deterministic participants are
  arithmetic. A room of language models may aggregate better or worse than
  this, and these numbers cannot tell you which.
- **Nothing about real tasks.** One synthetic task with a known best option and
  independent errors is the friendliest possible case for aggregation. Real
  disagreements are correlated, and correlated errors are exactly what pooling
  cannot fix.
- **Nothing about long rooms.** Conformity in a group of language models rises
  with interaction time. The budgets here are small on purpose, and a longer
  episode should be expected to buy correlated error rather than better
  judgement.

`tinyhivemind-hive` is a protocol for bounded deliberation with an auditable
termination reason. That is the whole claim.
