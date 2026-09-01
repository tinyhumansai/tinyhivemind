# The deliberation benchmark

A simulation harness for `tinyhivemind-hive`: it runs whole deliberation episodes
against reproducible synthetic rooms, scores them against two controls, sweeps
the episode policy, and reports what the library itself costs per step.

```sh
cargo run --release -p tinyhivemind-hive --example bench            # compare arms
cargo run --release -p tinyhivemind-hive --example bench -- --trace # one episode
cargo run --release -p tinyhivemind-hive --example bench -- --sweep # tune the policy
cargo run --release -p tinyhivemind-hive --example bench -- --swarm # several desks
cargo run --release -p tinyhivemind-hive --example bench -- \
  --agent-cmd "opencode run --pure -m openrouter/~openai/gpt-mini-latest"
```

This file documents the harness. The findings it produces, and what they do and
do not claim, are in [the benchmark write-up](https://github.com/tinyhumansai/tinyhivemind/wiki/Benchmarks).

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
| `hive+ref` | The tuned policy with `refutation_cap: Some(2)` — a cited fact can cap a hypothesis for the whole room. |
| `hive+ev` | The same, plus `require_evidential`: support counts only if its citation chain reaches a stated fact. |

The last two lose, reproducibly and by a lot, and the write-up in
[`docs/experiments/2026-09-01-refutation-and-grounds.md`](../../../../docs/experiments/2026-09-01-refutation-and-grounds.md)
says by how much and what the harness does not test. They are here because an
arm that cannot lose is not evidence, and a mechanism scored and reported is
worth more than one quietly shipped on.

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

It also **refutes** — before it objects — when the room is running a policy that
would let a refutation take effect and it rates an option on the floor clearly
below its own, by the same 60-point gap that separates the true option from a
decoy. A `None` or unreachable `refutation_cap` turns both the mechanism and the
move off together, so a control arm differs from its treatment in one thing
rather than in two.

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
hive+ref      8.99        88.6        75.0          2827         35398
hive+ev      10.29        60.8        55.9          2971         29816
```

The tuned deliberation beats the matched-budget control at half the budget, and
one responder off the ladder reaches 57.6%. The quorum threshold and the turn
budget are the two settings that decide this, the blind round is worth 24
points of accuracy on its own, and the state machine costs about 2.3 µs per
step.

The two refutation arms lose, which is why both knobs are off in
`QuorumPolicy::DEFAULT`. `hive+ref` falls below even the vote control, and
`hive+ev` starves the room — it fails to decide two episodes in five. [The benchmark write-up](https://github.com/tinyhumansai/tinyhivemind/wiki/Benchmarks)
has the tables behind each of those, across desk sizes, plus what the benchmark does not show.

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

### A real problem

Without a scenario the live room deliberates a brief with no answer, which
measures whether a model can hold the grammar and nothing else. `--scenario`
gives it a problem that has one:

```sh
cargo run --release -p tinyhivemind-hive --example bench -- \
  --agent-cmd "opencode run --pure -m openrouter/openai/gpt-5-mini" \
  --scenario crates/tinyhivemind-hive/examples/bench/scenarios/checkout-503.txt \
  --repeat 5
```

A scenario file is a shared brief, the options under the ids the room should
use, a private brief per member, and the recorded answer:

```text
task: what the room must decide
truth: the option id that is genuinely right

[option rollback]
One sentence describing it.

[agent planner]
role: release manager, who owns what can and cannot be shipped
knows: a fact this member holds and nobody else does
```

The private briefs are deliberately not appended to the shared journal. A fact
every member can already read is not private information, and a room whose
members all start from the same facts has nothing to pool.

Each round runs both arms against the same real agents: one deliberation
episode, then an independent poll of the same members answering alone, decided
by plurality and scored as no answer on a tie. `--repeat` runs the pair N
times, because a live room is sampled rather than computed and one episode is
an anecdote.

The scenario that ships here is a hidden profile — the shared brief plants the
wrong answer and the right one is reachable only by pooling facts across four
members. That shape is what lets the poll lose; a scenario whose answer
survives deleting every private brief measures nothing. Its header comment
records the two designs that failed that test before this one passed it.

Live mode asserts nothing and is not part of CI; it is for watching real agents
hold — or fail to hold — the trace grammar, and for watching whether a room
pools what its members separately know.
`crates/tinyhivemind-hive/tests/openrouter_hive_live.rs` is the asserting
version, behind the `e2e` feature.

## Several channels

`--swarm` runs a different experiment on the same machinery: not one room
deciding, but a **federation** of desks that cannot read each other's
transcripts and can only reach one another by a `referral`.

The task changes shape to make the boundary cost something. Each desk carries a
bias of its own — one option every member of that desk overrates, because they
read the same transcript and are wrong about the same thing. Within a desk that
bias is invisible: every member confirms every other, and averaging correlated
error does not cancel it. Across desks the biases are independent and do. So
the answer is reachable only by pooling across channels, which is the
multi-channel form of the hidden profile the live scenarios use, written in
numbers so it can be run ten thousand times.

`--bias` is bounded on both sides, and both bounds matter. Above the 60-point
gap between the true option and a decoy, a desk's own average points at the
wrong answer, so no amount of deliberating inside one channel finds the right
one. Below roughly `60 × desks`, the biases still cancel once every desk has
heard every other. Outside that window the experiment measures nothing, which
`--bias 0` and `--bias 160` both demonstrate.

| arm | what it is |
| --- | --- |
| `siloed` | The same desks, members and budgets, with referrals off. A desk can only talk to itself. |
| `swarm` | The same, with referrals on: two hops, desk mentions and returns. |
| `pooled` | The ceiling control. Every desk is handed every other desk's readings *for free* — no turn, no referral, no channel crossed — and then deliberates siloed. |
| `merged` | Every member of every desk on one desk, given the whole federation's budget. The control that removes the boundary rather than crossing it. |
| `vote` | One independent answer per member, decided by plurality. |

`pooled` is the arm that keeps the swarm honest. The swarm's members exchange
numeric readings, which the siloed members never get a chance to, and a reader
is entitled to ask how much of the difference is the *protocol* and how much is
simply having the numbers. Whatever `pooled` scores is what the information is
worth; whatever `swarm` scores below it is what the channel boundary still
costs after `referral` has done its work.

Every arm is charged for every agent invocation, including the ones a referral
causes on the far desk and on the way back. A member that spends its turn
asking another desk does not also get to argue in its own that turn.

### How a member decides to cross

A simulated member asks one question of each peer channel, before its desk has
backed anything: *I will not put an option on the floor on the strength of what
my own desk thinks, when nobody outside it has told me anything about it.*

The timing is the whole ballgame, and an earlier version of this harness got it
wrong. It asked *after* proposing — which sounds more natural — and every desk
committed to its own decoy with the correction sitting three lines below the
decision. A desk whose members share a bias reaches quorum inside its own blind
opening round, and an answer arriving after that is information the desk has
already voted past.

Asks are counted **per desk**, not per member. The answer lands in the desk's
own transcript where every member reads it, so a second member asking the same
desk the same question spends a turn to learn what it could have read.
`referral` bounds how deep a chain goes; bounding how wide one desk may go is
the host's job.

What crosses is **information, never a vote**. The message that lands on the
far desk is `!evidence`, which adds no supporter to any topic: the members
there hear another channel's reading, average it into their own, and still have
to spend their own turns saying so before anything is counted.

### A live federation

```sh
cargo run --release -p tinyhivemind-hive --example bench -- --swarm \
  --scenario crates/tinyhivemind-hive/examples/bench/scenarios/checkout-503-federated.txt \
  --agent-cmd "claude -p --model sonnet"
```

The scenario file grows `[desk ...]` sections and a `desk:` line per agent.
`checkout-503-federated.txt` is the single-room hidden profile with its facts
split across three desks, so the conjunction that makes the answer is not
merely spread across members — it is spread across *rooms*.

In the live arm the harness writes no mention on anybody's behalf. Each agent
is told which channels exist and how to address one, and decides for itself
whether to spend its turn asking. The line it writes is read by the real
mention grammar and routed by the real `referral` fold, exactly as the
simulated ask is. A run in which nothing crosses is a finding about the agents
rather than a failure of the harness.

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
| `--swarm` | run a federation of desks instead of one room |
| `--desks N` | channels in the federation, 2–4 (default 3) |
| `--per-desk N` | members on each channel, 2–8 (default 4) |
| `--bias N` | how much a desk overrates its own decoy (default 110) |
| `--agent-cmd CMD` | drive one episode through a real agent CLI |
| `--scenario PATH` | give the live room a real problem with private facts |
| `--repeat N` | run a live scenario N times and count both arms |

`--swarm --trace` prints the interleaved multi-channel transcript, and
`--swarm --noise` defaults to ±50 rather than ±90: at the single-room default
the desk bias is swamped, every desk is individually unbiased, and crossing a
channel would be measuring nothing.

## Layout

| file | what it holds |
| --- | --- |
| `main.rs` | the command line, the tuned policy, the modes, and the tables |
| `sim.rs` | the rooms, the private evaluations, and what a participant says |
| `federation.rs` | several desks, each with a correlated bias of its own |
| `swarm.rs` | one journal per channel, the scheduler, and the referral edge |
| `run.rs` | the host: a journal, a roster, and the step loop |
| `arms.rs` | the `ladder`, `vote`, `merged` and federated controls |
| `sweep.rs` | the policy grid and its ranking |
| `metrics.rs` | aggregation and formatting |
| `live.rs` | the external agent CLI backend, its prompt, and the solo poll |
| `scenario.rs` | the scenario file format, the briefs, and the recorded answer |
| `scenarios/` | the scenario files themselves |
| `rng.rs` | a seeded `SplitMix64`, so every run reproduces |
