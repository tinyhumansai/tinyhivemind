# The deliberation benchmark

A simulation harness for `tinyhivemind-hive`: it runs whole deliberation episodes
against reproducible synthetic rooms, scores them against two controls, sweeps
the episode policy, and reports what the library itself costs per step.

```sh
cargo run --release -p tinyhivemind-hive --example bench            # compare arms
cargo run --release -p tinyhivemind-hive --example bench -- --trace # one episode
cargo run --release -p tinyhivemind-hive --example bench -- --sweep # tune the policy
cargo run -p tinyhivemind-hive --example bench -- --agent-cmd "opencode run"
```

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
| `hive+` | The same, at the policy `--sweep` picks. |

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
ladder        1.00       100.0        57.6          1086        920481
vote         12.00       100.0        78.2             0           inf
hive          6.16        89.7        73.3          2257         61920
hive+         6.73        98.3        81.5          2222         58249
```

Three things are worth reading out of it.

**The deliberation beats the matched-budget control, at half the budget.**
`hive+` decides correctly 81.5% of the time in 6.7 turns; the control needs all
12 and reaches 78.2%. A single responder — today's behaviour — manages 57.6%.
The margin over the control is small and it is meant to be: independent
sampling plus a plurality is most of what a room is for, and a protocol that
could not clear it would not be worth its budget.

**The quorum threshold is the load-bearing knob.** Five members can put two
grounded supporters behind each of two options, and an episode where two options
both carry is deadlocked by definition. A threshold above half the desk makes
that unreachable: the sweep's best policy takes the deadlock rate from 514/5000
to zero, and the decision rate from 89.7% to 98.3%. Hosts should set `QuorumPolicy::threshold` above half the desk.

**The blind round is not decoration.** Rerun with `--no-blind` and accuracy
collapses to 58.0% — level with a single agent — because the room cascades onto
whatever was proposed first. That is an information cascade, and `Visibility`
is what prevents it.

None of this is evidence that language models deliberate better in a room. The
participants here are arithmetic. What it does show is which *policy*
aggregates information and which throws it away, which is the question a host
has to answer when it configures a desk.

## Cost

`ns/step` measures the library alone, with the participants' own time excluded:
about **2.2 µs** to decide who speaks next over a live transcript, or roughly
58,000 whole episodes per second on one core. A model turn is six orders of
magnitude more expensive, so the protocol is free in any real deployment.

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

A run of the first command converged on `#rollout-strategy` in 11 turns and
about 30 seconds of wall clock. Three things showed up that the simulation
cannot:

- **Topics drift.** One model proposed `#rollout`, another `#rollout-strategy`,
  for the same idea, and the support behind them did not add up. A host running
  live rooms should seed the topic vocabulary rather than let each turn coin
  one.
- **Models repeat themselves.** Four consecutive turns restated the same
  `!question` verbatim. `repetition_cap` damps a restated *support*, so it does
  not catch this; a host that cares can spend the budget elsewhere.
- **Quorum above half the desk needs a desk to spare.** At `--agents 3` the
  tuned threshold of 3 is unanimity, and a single `!object` silences one
  advocate permanently — that episode ran out of budget instead of deciding.
  The rule is a threshold above half the desk *and* a margin over it, which
  five members and a threshold of three have.

Live mode asserts nothing and is not part of CI; it is for watching real agents
hold — or fail to hold — the trace grammar.
`crates/tinyhivemind-hive/tests/openrouter_hive_live.rs` is the asserting
version, behind the `e2e` feature.

## Flags

| flag | meaning |
| --- | --- |
| `--episodes N` | rooms to simulate (default 500) |
| `--agents N` | members per room, 2–8 (default 5) |
| `--topics N` | options on offer, 2–8 (default 4) |
| `--noise N` | half-width of the error on a private evaluation (default 90) |
| `--seed N` | room generator seed (default 1) |
| `--budget N` `--quorum N` `--window N` | episode policy |
| `--dominance N` `--repetition N` `--no-blind` | episode policy |
| `--trace` | print one episode turn by turn |
| `--sweep` | score the 96-point policy grid |
| `--agent-cmd CMD` | drive one episode through a real agent CLI |

## Layout

| file | what it holds |
| --- | --- |
| `main.rs` | the command line, the modes, and the tables |
| `sim.rs` | the rooms, the private evaluations, and what a participant says |
| `run.rs` | the host: a journal, a roster, and the step loop |
| `arms.rs` | the `ladder` and `vote` controls |
| `sweep.rs` | the policy grid and its ranking |
| `metrics.rs` | aggregation and formatting |
| `live.rs` | the external agent CLI backend |
| `rng.rs` | a seeded `SplitMix64`, so every run reproduces |
