# The deliberation benchmark

A simulation harness for `tinyhivemind-hive`: it runs whole deliberation episodes
against reproducible synthetic rooms, scores them against two controls, sweeps
the episode policy, and reports what the library itself costs per step.

```sh
cargo run --release -p tinyhivemind-hive --example bench            # compare arms
cargo run --release -p tinyhivemind-hive --example bench -- --trace # one episode
cargo run --release -p tinyhivemind-hive --example bench -- --sweep # tune the policy
cargo run --release -p tinyhivemind-hive --example bench -- \
  --agent-cmd "opencode run --pure -m openrouter/~openai/gpt-mini-latest"
```

This file documents the harness. The findings it produces, and what they do and
do not claim, are in [`docs/benchmark.md`](../../../../docs/benchmark.md).

## The task

A room of five agents chooses between four options, exactly one of which is
genuinely best. Every member holds a *private, noisy* evaluation of every
option — the true quality plus a uniform error of half-width `--noise` — so no
member is individually reliable, and the room's only route to the right answer
is to pool what its members separately believe.

Everything is seeded. The same `--seed` produces the same rooms, the same
private evaluations, and the same transcripts, so a change in a reported number
is a change in the library rather than in the weather.

## The arms

Every arm decides the same rooms from the same private evaluations.

| arm | what it is |
| --- | --- |
| `ladder` | Today's behaviour: `responder_plan` in `tinyhivemind` selects one responder off the real ladder and that agent answers alone. One turn. |
| `vote` | The honest matched-budget control — independent answers decided by plurality, nobody seeing anybody. It is given the *whole* budget, more turns than the deliberation actually spends. |
| `hive` | A deliberation episode at `EpisodePolicy::DEFAULT`. |
| `hive+` | The same, at the tuned policy: a majority quorum that is never unanimity, and three turns of budget per member. |

A multi-agent result without a matched-budget control is close to meaningless,
because the multi-agent arm has usually just spent more compute. `vote` is that
control, and it is a strong one: independent sampling plus a plurality already
recovers much of what a room is for.

## What the participants do

The simulated participants are mechanical, which is the point — a language
model would make the numbers unreproducible and would confound protocol quality
with model quality. On its turn a participant, seeing exactly what
`project_for` allowed it to see:

1. **breaks a deadlock** — if two options both carry, it objects to a message
   advocating the one it rates lower. Adding support cannot resolve that state,
   because both options stay above the threshold no matter how much weight one
   gains; silencing an advocate can, which is why the objection names a
   *message* rather than a topic;
2. **commits** — in `Phase::Commit`, records what the room actually carried;
3. **supports** — backs the option on the floor it rates highest once each
   independent peer backing it is weighed against its own private signal. This
   is the step that pools information;
4. **closes** — if the leading option is one supporter short of quorum and is
   not clearly worse than its own choice, it backs that instead of holding out;
5. **proposes** — puts its own favourite on the floor if nobody has;
6. **objects, or adds evidence**, according to its role.

It reads the medium through the library's own `resolve` and `standings`, not
through a private imitation of them, and it emits ordinary prose 6% of the time
so the benchmark measures the protocol rather than a formatter.

## Results

5000 rooms, 5 agents, 4 options, `--noise 90`, on one core:

```text
arm       turns/ep   decided %   correct %       ns/step    episodes/s
ladder        1.00       100.0        57.6          1109        901660
vote         15.00       100.0        78.5             0           inf
hive          6.16        89.7        73.3          2231         62637
hive+         6.75        99.4        82.1          2278         56641
```

The tuned deliberation beats the matched-budget control at half the budget, and
one responder off the ladder reaches 57.6%. The quorum threshold and the turn
budget are the two settings that decide this, the blind round is worth 24
points of accuracy on its own, and the state machine costs about 2.3 µs per
step. [`docs/benchmark.md`](../../../../docs/benchmark.md) has the tables
behind each of those, across desk sizes, plus what the benchmark does not show.

## Live mode

`--agent-cmd` swaps the simulated participants for a real agent CLI — one
process per turn, any command that takes a prompt as its final argument and
prints an answer:

```sh
cargo run --release -p tinyhivemind-hive --example bench -- \
  --agent-cmd "opencode run --pure -m openrouter/~openai/gpt-mini-latest" --agents 5
```

```sh
cargo run -p tinyhivemind-hive --example bench -- --agent-cmd "claude -p"
cargo run -p tinyhivemind-hive --example bench -- --agent-cmd "codex exec"
```

The library still authorizes exactly one speaker per step, so the number of
processes an episode can start is bounded by its turn budget and by nothing
else. Coloured output and a banner are stripped before the marker line is read.

The prompt is not a static block of protocol text, because live rooms fail in
ways the simulation cannot reach. It names the options already on the floor
with their standings, folded through the library's own `standings`, so support
does not split across two names for one idea; it shows a participant its own
last line, because models restate it verbatim; it offers only the moves that
count in the turn's phase, because a `!commit` written during deliberation adds
no supporter; and it calls out the grammar's `#` and `^` sigils, because models
drop them. Each of those is a host obligation rather than something the library
can impose, and each was found by running the thing.

Live mode asserts nothing and is not part of CI; it is for watching real agents
hold — or fail to hold — the trace grammar.
`crates/tinyhivemind-hive/tests/openrouter_hive_live.rs` is the asserting
version, behind the `e2e` feature.

## Flags

| flag | meaning |
| --- | --- |
| `--episodes N` | rooms to simulate (default 500) |
| `--agents N` | members per room, 2–8 (default 5); moves the tuned quorum and budget with it |
| `--topics N` | options on offer, 2–8 (default 4) |
| `--noise N` | half-width of the error on a private evaluation (default 90) |
| `--seed N` | room generator seed (default 1) |
| `--budget N` `--quorum N` `--window N` | episode policy, overriding the tuned values |
| `--dominance N` `--repetition N` `--no-blind` | episode policy |
| `--trace` | print one episode turn by turn |
| `--sweep` | score the policy grid, swept relative to the desk size |
| `--agent-cmd CMD` | drive one episode through a real agent CLI |

## Layout

| file | what it holds |
| --- | --- |
| `main.rs` | the command line, the tuned policy, the modes, and the tables |
| `sim.rs` | the rooms, the private evaluations, and what a participant says |
| `run.rs` | the host: a journal, a roster, and the step loop |
| `arms.rs` | the `ladder` and `vote` controls |
| `sweep.rs` | the policy grid and its ranking |
| `metrics.rs` | aggregation and formatting |
| `live.rs` | the external agent CLI backend and its prompt |
| `rng.rs` | a seeded `SplitMix64`, so every run reproduces |
