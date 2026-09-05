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
| `hive+dir` | The tuned policy with `directory: Some(DirectoryPolicy::DEFAULT)` — the folded transactive-memory directory on, so `BidReason::Knows` is reachable. |
| `hive+defer` | The tuned policy with `defer_cap: Some(N)` and no directory: members may stand aside on a topic that is not theirs, with nothing routing the vacated turn. |
| `hive+dir+defer` | Both at once, which is the arrangement `docs/specs/expert-delegation.md` describes end to end. |
| `ladder+dir` | The responder ladder again, with a directory the room *earned* over `--history` prior episodes of `hive+` on the same room. The selector's candidates carry that directory's per-agent lines as their `description`, the request names the topic the call turns on, and a router that reads the descriptions picks the heaviest holder of it. Validated through the real `accept_selection`. |
| `hive+cost` `all-reasoning` | Only under `--cost-tiers`, in the cost table: the delegating room against one that puts every seat on the expensive tier. |

The six rows above `hive+dir` are the published table; the delegation arms are
appended rather than interleaved, so `--seed 1 --episodes 5000` still prints
them byte for byte.

`hive+ref` and `hive+ev` lose, reproducibly and by a lot, and the write-up in
[`docs/experiments/2026-09-01-refutation-and-grounds.md`](../../../../docs/experiments/2026-09-01-refutation-and-grounds.md)
says by how much. The delegation arms mostly draw, and
[`DELEGATION.md`](DELEGATION.md) says so. They are all here because an arm that
cannot lose is not evidence.

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
rather than in two. It **defers** only under `--defer-cap`, on a topic it knows
another member owns.

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
hive+dir      6.75        99.4        82.1          2134         60458
hive+defer    6.75        99.4        82.1          1942         66456
hive+dir+defer 6.75        99.4        82.1          1995         64680
ladder+dir    1.00       100.0        49.5           735       1359856
```

The three delegation arms score exactly what `hive+` scores, which is what the
specification predicted for a room of uniform expertise: with nothing to route
on, a directory routes nowhere. `ladder+dir` is eight points *worse* than the
uninformed ladder. [`DELEGATION.md`](DELEGATION.md) says why.

The tuned deliberation beats the matched-budget control at half the budget, and
one responder off the ladder reaches 57.6%. The quorum threshold and the turn
budget are the two settings that decide this, the blind round is worth 24
points of accuracy on its own, and the state machine costs about 2.3 µs per
step.

The two refutation arms lose, which is why both knobs are off in
`QuorumPolicy::DEFAULT`. `hive+ref` falls below even the vote control, and
`hive+ev` starves the room — it fails to decide two episodes in five. [The benchmark write-up](https://github.com/tinyhumansai/tinyhivemind/wiki/Benchmarks)
has the tables behind each of those, across desk sizes, plus what the benchmark does not show.

## Statistics

A second table is printed under the first, in `metrics.rs`:

```text
arm       correct %          95% CI  expert %   to-expert   route %   cost/ep     rho
ladder         57.6       56.2–59.0         —           —         —      1.00       —
vote           78.5       77.4–79.6         —           —         —     15.00       —
hive           73.3       72.0–74.5         —           —         —      6.16     0.7
hive+          82.1       81.0–83.1         —           —         —      6.75     0.7
```

`95% CI` is a Wilson score interval on `correct %`, chosen over the plain
normal approximation because several arms here land near 0% or 100%, where the
plain interval can cross outside `[0, 100]` and its coverage is worst. Under
each row, a paired-bootstrap comparison line reports the same arm against
`vote` at equal turns, e.g. `hive+ − vote: +3.6 [+2.1, +5.0]`: the accuracy
difference and its 95% interval, resampling episode indices together for both
arms because they decided the *same* rooms. `expert %` and `to-expert` report
how often, and how late, the room's decisive member — a `--specialists` topic
expert or the `--hidden-profile` fact-holder — spoke at all, over the episodes
that *had* one. `route %` is the share of those episodes in which the
responder ladder picked that member as its one responder. `rho` is the
circularity number `docs/specs/expert-delegation.md` obliges this benchmark to
print: at the end of every episode the harness folds `directory()` over that
episode's own journal — always at `DirectoryPolicy::DEFAULT`, whatever the arm
asked for, so the number is comparable across arms — and takes the Spearman
rank correlation between each member's total directory weight and the number
of turns it took, averaged over the sample. A column reads `—` rather than
`0.0` wherever the arm structurally cannot produce that number (a control arm
never deliberates, so it never has an expert or a rho; a uniform room names no
expert, so nothing can be routed right or wrong).

**What `rho` means.** Near `1.0` the directory reproduces the speaking order
and has learned nothing except who talked — the failure
`docs/research/delegation.md` names *who spoke becomes who is thought to
know*. At or below zero it is reading grounded deposits and other members'
citations rather than turn count. The simulated rooms sit at about `0.7` on
every deliberating arm and every expertise shape, which bounds how much any
result here can be credited to the directory having found something. It is
printed beside accuracy rather than in a footnote for exactly that reason.

`--json` prints one flat JSON object per arm, one per line, ahead of both
tables, covering every column of both plus `expert_led` — the share of
episodes in which the decisive member authored the first `!propose` for the
topic the room went on to decide, which the tables have no room for. `--stats-check` runs a small set of
known cases through `wilson`, `paired_bootstrap` and `spearman_milli` — the
statistics live in an example, so `cargo test` never exercises them — and
exits `0` or `1`.

## Delegation

`--specialists`, `--hidden-profile`, `--defer-cap`, `--history` and
`--cost-tiers` measure whether expert delegation earns its place: whether the
floor reaches the member holding the deciding fact, how precisely a router
routes, and what accuracy costs per unit spent. The arms, the numbers they
scored, and why the hidden profile is decided before delegation can act are in
[`DELEGATION.md`](DELEGATION.md).

## Live mode

`--agent-cmd` swaps the simulated participants for a real agent CLI, one
process per turn, and `--api-base` posts each turn straight to an HTTP
endpoint through `curl` instead. `--scenario` gives the live room a real
problem with a recorded answer and private facts per member, and `--repeat`
runs it several times. The prompt, the scenario file format, the two backends
and what running them actually turned up are in [`LIVE.md`](LIVE.md).

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
| `--specialists N` | `N` members each read one topic far more tightly than everybody else, and everybody else's read of that topic widens to match -- information is redistributed, not created |
| `--hidden-profile` | one decoy is planted above every member's own argmax except one member, who alone holds the fact that rules it out |
| `--cost-tiers` | with `--specialists`, a specialist's own turn costs ten times a lay member's, for the `cost/ep` column |
| `--defer-cap N` | turns a member may spend deferring to a topic's expert instead of arguing outside its own specialty (default 1, minimum 1); read by `hive+defer` and `hive+dir+defer` |
| `--history N` | prior episodes of `hive+` the `ladder+dir` arm earns its directory from (default 3) |
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
| `--json` | print one flat JSON object per arm, ahead of the tables |
| `--stats-check` | run the statistics module's self-check and exit `0` or `1` |
| `--timeout SECS` | per-turn deadline for a live agent or HTTP request (default 180) |
| `--api-base URL` | drive seats directly over HTTP instead of a CLI |
| `--api-key-env NAME` | env var carrying the HTTP backend's key (default `LADDER_API_KEY`) |
| `--model NAME` | the HTTP backend's default model (default `flash`) |
| `--wire openai\|anthropic` | which chat wire format the HTTP backend speaks (default `openai`) |
| `--model-cost model=N` | cost per 1000 tokens for `model`, for the usage table (repeatable) |
| `--seat-model agent_id=model` | per-seat model override for the HTTP backend (repeatable) |
| `--seat-cmd agent_id="command"` | per-seat command override for the CLI backend (repeatable) |
| `--specialist-model NAME` | model seated for a member the scenario marks `expert_on:` or `tier: reasoning` |
| `--thinking on\|off` | whether the HTTP backend reasons before answering (default `on`) |

`--swarm --trace` prints the interleaved multi-channel transcript, and
`--swarm --noise` defaults to ±50 rather than ±90: at the single-room default
the desk bias is swamped, every desk is individually unbiased, and crossing a
channel would be measuring nothing. `--hidden-profile --noise` defaults to ±50
for the same reason and by the same rule — an explicit `--noise` still wins —
so that the planted decoy, which reads 190 against the true option's 100, is
every non-decisive member's own argmax and the matched-budget poll scores
zero by construction.

## Layout

| file | what it holds |
| --- | --- |
| `main.rs` | the command line, the tuned policy, the modes, and the tables |
| `sim.rs` | the rooms, the private evaluations, what a participant says, and the `Expertise` shapes (`--specialists`, `--hidden-profile`) that redistribute those evaluations |
| `federation.rs` | several desks, each with a correlated bias of its own |
| `swarm.rs` | one journal per channel, the scheduler, and the referral edge |
| `run.rs` | the host: a journal, a roster, and the step loop |
| `arms.rs` | the `ladder`, `vote`, `merged` and federated controls |
| `sweep.rs` | the policy grid and its ranking |
| `metrics.rs` | aggregation, formatting, and the confidence-interval, bootstrap and rank-correlation statistics |
| `live.rs` | the shared prompt state, the external agent CLI backend, and the solo poll |
| `http.rs` | the direct-HTTP backend: the same prompt state over `curl`, and its usage table |
| `scenario.rs` | the scenario file format, the briefs, and the recorded answer |
| `scenarios/` | the scenario files themselves |
| `DELEGATION.md` | the delegation arms, the three questions they answer, and what they scored |
| `LIVE.md` | live rooms: the prompt, the scenario format, and the CLI and HTTP backends |
| `rng.rs` | a seeded `SplitMix64`, so every run reproduces |
