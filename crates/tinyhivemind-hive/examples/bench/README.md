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
  --scale-sweep --hidden-profile          # room size against channel topology
cargo run --release -p tinyhivemind-hive --example bench -- \
  --swarm --desks 100 --per-desk 10 --topics 128   # a thousand agents
cargo run --release -p tinyhivemind-hive --example bench -- \
  --agent-cmd "opencode run --pure -m openrouter/~openai/gpt-mini-latest"
```

Every sample loop runs across cores; `--jobs` bounds it and changes wall clock
and nothing else. Running at a thousand agents — the size ceilings, the
`--ask-cap` knob that decides whether a large federation decides anything, and
what a big room actually costs — is [`SCALE.md`](SCALE.md).

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
| `hive+aside` | The tuned policy with `--aside-cap`: a member that cannot separate its two best options spends a turn asking one peer for its reading, privately. Loses; see [the write-up](../../../../docs/experiments/2026-09-07-why-asides-lose.md). |
| `hive+ask` | The identical exchange in the open — same turns, same words, every member reads it. The control that isolates *privacy* from *asking*. |
| `hive+aside!` | The private check again, aimed at whoever the transcript shows has grounded the option rather than at whoever spoke first. |
| `hive+fact` | The aimed check again, carrying the fact that rules an option out rather than a number to be averaged into an error the pair may share. |
| `hive+mute` | The identical check on the identical turns, with the answer **discarded**. Targets the way `hive+aside`/`hive+ask` do — whoever spoke first — so it is a matched-turn control for those two: what it loses against `hive+` is what the turns cost. Against `hive+fact`/`hive+aside!`, which are aimed at the fact-holder, the comparison also carries a targeting difference, not content value alone. |
| `hive+along` | The aimed, fact-carrying check again, **riding alongside** the member's floor move rather than replacing it: one authorized turn, two rows, and the episode unable to see the second. Unlike `hive+fact°` it picks its peer from the transcript exactly as `hive+fact` does, so it is the clean isolation of scheduling. See [ADR 0011](../../../../docs/adr/0011-an-aside-rides-alongside-a-turn.md). |
| `hive+hush` | `hive+share` with the answers **discarded**: the same rows on the same turns, consuming the same sequence numbers, saying nothing. Alongside rows land unevenly, so this is the control that says whether that unevenness moves the room by itself. |
| `hive+share` | The same free row spent **continuously** — a contact on every turn, to a peer not yet reached, carrying a reading of every option rather than an answer to one. What a colony's contacts actually look like, and what only a free row can afford. |
| `hive+rounds` | The same continuous exchange run **off the floor**, in rounds between turns: nobody takes the floor for it, so its volume is set by `--exchange-cap` rather than by how many turns the room takes. Priced in the `calls/ep` column. See [ADR 0012](../../../../docs/adr/0012-an-exchange-round-spends-model-calls-not-turns.md). |
| `hive+quiet` | `hive+rounds` with the answers **discarded**: the same rounds, the same rows, the same sequence numbers consumed, and no information transferred. The control that separates what an off-floor exchange *says* from what merely writing its rows does to salience decay. |
| `hive+fact°` | The same bounded exchange as `hive+fact`, held **off the floor** — before the episode opens, spending no turn the room could have deliberated with. Its peer is chosen from private room state rather than the transcript, so it bounds what the exchange is worth off the floor rather than isolating scheduling alone. |
| `swarm°` | Only under `--swarm`: the same federation and the same referral policy as `swarm`, with the question asked **off the floor** and bounded by `--ask-cap` instead of spending the authorized turn. It is what keeps a federation above eight desks deciding anything at all — see [`SCALE.md`](SCALE.md). |
| `hive+pooled` | The **ceiling for equal-weight pooling**: every private reading and every fact already in every member's hands, free, averaged with no regard for whose reading it is. No amount of pairwise exchange beats it on the rooms this benchmark measures (uniform and hidden-profile, where every peer's reading is equally reliable) — under `--specialists`, where readings genuinely differ in reliability, a protocol that could tell them apart could in principle beat indiscriminate averaging. |
| `ladder+dir` | The responder ladder again, with a directory the room *earned* over `--history` prior episodes of `hive+` on the same room. The selector's candidates carry that directory's per-agent lines as their `description`, the request names the topic the call turns on, and a router that reads the descriptions picks the heaviest holder of it. Validated through the real `accept_selection`. |
| `all-reasoning` | Only under `--cost-tiers`, in the cost table: `hive+dir+defer` (the delegating room) against a policy that puts every seat on the expensive tier. |

The six rows above `hive+dir` are the published table; the delegation arms are
appended rather than interleaved, so `--seed 1 --episodes 5000` still prints
them byte for byte.

Every aside arm that spends a turn loses, and the seven of them together
settle what the loss is made of. `hive+aside − hive+ask` spans zero in every
configuration, so privacy is never the variable. `hive+mute` discards the
answer and loses *more* than `hive+aside` and `hive+ask` -- the two arms it is
a clean matched-turn control for, since all three target the same way -- so
against those two the content is never the variable either; the cost is the
turns, in full, before a word changes hands. `hive+fact` lands within a point
of `hive+mute` too (−16.9 against −15.7), but `hive+fact` is aimed at the
fact-holder while `hive+mute` is not, so that particular gap also carries a
targeting difference and should not be read as content value alone.
`hive+aside!` is a sharper version of the same point: aimed at the actual
fact-holder it loses `-17.1`, *more* than `hive+mute`, because conscripting the
one member whose public turn matters into an audience of one is itself a cost.
`hive+fact°` runs
the same bounded exchange off the floor and moves from −16.9 to
`+3.2 [+2.1, +4.2]` on a hidden profile, so scheduling accounts for most of the
gap — though `hive+fact°` picks its peer from private room state rather than
`hive+fact`'s transcript-only `View::grounded_by`, so this is an upper bound on
the off-floor benefit, not a pure isolation of scheduling from targeting. And
`hive+pooled` beats `hive+` by `+30.8 [+28.7, +32.9]` in *fewer* turns: peer
information is worth more here than anything else this benchmark measures, and
buying it one floor turn at a time is what costs more than it is worth.

So the accounting changed. `hive+along` writes the identical exchange alongside
each member's floor move instead of in place of one, and moves from `-16.9` to
`+0.5 [+0.1, +0.9]` — a 17.4-point swing bought by charging the room nothing.
`hive+share` then spends the free row continuously and reaches
`+1.4 [+0.4, +2.4]`. Both stay inside the turn contract: only the member the
library authorized ever writes, and it writes at most one private row per turn.

`hive+rounds` takes the floor out of it altogether — members contact each other
in rounds *between* turns, bounded by the library's `exchange` fold rather than
by how talkative the room is — and reaches `+3.2 [+1.8, +4.5]` at forty-five model
calls an episode. That is not free, and `calls/ep` is there so the price sits
beside the gain. Every one of these arms is a null on uniform rooms: a room whose
members differ only by independent noise has no concentrated information for a
contact to move.
[`docs/experiments/2026-09-07-why-asides-lose.md`](../../../../docs/experiments/2026-09-07-why-asides-lose.md)
carries the numbers and the argument;
[`2026-09-07-do-asides-help.md`](../../../../docs/experiments/2026-09-07-do-asides-help.md)
is the first pass, whose explanation of the hidden-profile loss it retracts.

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

Under `--aside-cap` it also **checks**: a member whose two best options sit
within half the true-to-decoy gap of each other cannot tell them apart, so it
spends a turn asking one peer what that peer reads, the peer spends a turn
answering, and the asker averages the answer into its own view through the same
`import` the federation uses for a reading that crossed a channel. The exchange
adds no supporter — an `!aside` line parses to no trace whatever its audience —
so a member still has to spend a further turn saying what it now thinks before
the room counts anything.

Under `--blind-evidence` it **deposits first**: its opening turn, while the
room is blind, states its own reading of the topic it knows best rather than
putting an option on the floor, and once the floor exists it proposes what it
now rates highest rather than backing a worse option that got there first. Its
support then cites the deposit the room actually stated about that option. See
[The evidence-first opening](#the-evidence-first-opening).

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

The second table printed under the first — `correct %`, its confidence
interval, `fact %`, `to-fact`, `knows %`, `defers/ep`, `route %`, `cost/ep`
and `rho` — is documented in [`STATISTICS.md`](STATISTICS.md), together with
the paired bootstrap and what its intervals do and do not license.

## The evidence-first opening

`--blind-evidence` changes one thing about the *participants* and nothing about
the library: while the room is still `Visibility::Blind`, a member's first turn
deposits `!evidence #topic` — its own reading of the topic it knows best, with
no citation, because nothing is visible to cite — instead of proposing an
option. Proposals begin once the room goes to `Visibility::Full`.

The finding it exists to state is short: **without an evidence-first opening, a
room whose members share a bias reaches quorum inside the blind round, and no
floor mechanism can act.** A `!propose` counts as a supporter, so four members
who privately favour the same planted decoy carry it before anybody has read
anybody; the episode's first non-blind turn is a commit turn, and a fact
arriving then has nothing left to change. That is not a hypothesis — the live
rooms recorded it (in every correct episode of the 2026-09-01 run the five
blind turns were five `!evidence` lines, one per member) and the federation
reached it from the other side (moving a desk's question to *before* it had
backed anything was the difference between failing outright and 77.5%).

It is off by default, so every published number that does not ask for it is
unchanged, and what it buys is measured rather than assumed:

```text
5000 rooms                        hive+   hive+dir   vote   ladder   ladder+dir
uniform                            82.1       82.1   78.5     57.6         49.5
uniform  --blind-evidence          75.3       75.7   78.5     57.6         98.8
--specialists 2                    74.2       74.2   71.1     52.6         45.1
--specialists 2 --blind-evidence   67.6       68.0   71.1     52.6         84.4
--hidden-profile                   15.3       15.3   15.0     35.1         34.6
--hidden-profile --blind-evidence  66.3       65.8   15.0     35.1         64.1
```

On an ordinary room it **costs** about seven points: five of the fifteen turns
go on deposits nobody needed, and `hive+` fails to decide 7% of the time rather
than 0.6%. On the hidden profile it is the difference between 15% and 66%. That
is the trade, stated rather than tuned away.

Two side effects are worth reading before the numbers are:

- **`ladder+dir` on a uniform room is an artifact, not a result.** The arm
  tells its router which *topic* the call turns on, and that topic is the
  correct option. With an evidence-first opening the directory records "who
  deposited a reading of `#truth`", and a member who deposited on `#truth` is
  usually a member whose favourite *is* `#truth` — so routing to the heaviest
  holder returns the right answer 98.8% of the time by construction. The
  92-point swing from the same arm's 49.5% under the ordinary opening is the
  size of the leak, not the size of the mechanism. Read the `--specialists`
  row instead, where the deposit is a specialist's tight reading rather than a
  vote, and even there read it knowing the topic was named.
- **The two refutation arms fall further.** `hive+ref` and `hive+ev` lose about
  twenty-five points under the flag. A blind round spent depositing is a blind
  round not spent proposing, and both arms already had the tightest budget.

## Delegation

`--specialists`, `--hidden-profile`, `--defer-cap`, `--history`,
`--cost-tiers` and `--blind-evidence` measure whether expert delegation earns
its place: whether the deciding fact reaches the floor in time, how precisely a
router routes, and what accuracy costs per unit spent. The arms, the numbers
they scored, and what makes `BidReason::Knows` reachable at all are in
[`DELEGATION.md`](DELEGATION.md).

`scenarios/index-lock-expert.txt` is the hidden profile written for a room with
a named specialist. The brief plants the decoy — a migration finished at 02:14
and write latency stepped at 02:14 — and three members' private facts back it.
Only `planner` holds the fact that kills it: the migration is forward-only and
rolling it back destroys paid orders. The answer, `#batch`, is a conjunction of
two halves that are inert apart. `scout` reports that the rebuild dropped the
table's lock escalation threshold from 50,000 to 5,000 and cannot see how large
any job's transactions are; `dba` reports the two batch sizes, 6,200 and 500,
neither of which has changed in a year. Two bulk writers are on the option list
precisely so that the threshold alone accuses both and the sizes alone accuse
neither. Polled alone against a live model the scenario answers `#rollback`:
three runs, two models, plurality wrong every time and never more than one
member on the truth. `scenarios/index-lock-tiers.txt` is the same scenario with
a `tier:` on every seat — four `cheap` seats whose job is a lookup and a
negation, one `reasoning` seat on `dba`, the only member asked to hold a number
against somebody else's threshold — and it exists to price that routing rather
than to change the answer.

## Live mode

`--agent-cmd` swaps the simulated participants for a real agent CLI, one
process per turn, and `--api-base` posts each turn straight to an HTTP
endpoint through `curl` instead. `--scenario` gives the live room a real
problem with a recorded answer and private facts per member, and `--repeat`
runs it several times. The prompt, the scenario file format, the two backends
and what running them actually turned up are in [`LIVE.md`](LIVE.md).

Ten backend rows have been run end to end — the HTTP backend on `flash`, on a
reasoning model, mixed-tier, `claude -p`, `opencode run`, and `codex exec`
against OpenRouter — over twenty-seven rounds. The headline is that **the
matched-budget poll found the answer in none of them**, while the rooms scored
9 of 27, and that the scenario's `truth_expert` spoke before the commit in
every round that had one and fourteen of those twenty-three rooms were still
wrong (subject to a caveat on that metric in `DELEGATION.md`). `!defer` was
used on none of the 266 turns and no turn was awarded on `BidReason::Knows`.
The `codex`
row is a CLI row by necessity: the router cannot relay a streaming Responses
request, so that model cannot go through `--api-base` at all and is driven with
`codex exec -c model_provider=…` pointed straight at OpenRouter. Every row and
what the transcripts show is in
[`DELEGATION.md`](DELEGATION.md#the-live-matrix).

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

## The context budget

Every arm above moves information for free, and that is a misleading
*engineering* ceiling once a participant is a language model with a bounded
window. `--context-sweep` charges for it: each arm runs across a ladder of
window capacities that evicts rows from the middle and discounts survivors by
a U-curve. `hive+pooled` — the arm that looks unbeatable — loses a third of
its lead once its window matches its own payload; `hive+` and `hive+along`
barely notice, because they barely use the window. No squeeze makes the
deliberating room the better choice on this task, so the honest reading is
narrower than "context economy vindicates deliberation": fix the protocol
first. The full numbers, the two things this experiment did and did not find,
and why the sweep reports an ordering rather than a value are in
[`CONTEXT.md`](CONTEXT.md).

## Flags

| flag | meaning |
| --- | --- |
| `--episodes N` | rooms to simulate (default 500) |
| `--agents N` | members per room, 2–256 (default 5); moves the tuned quorum and budget with it. The ceiling is what this harness will spend, not a library limit — `tinyhivemind-hive` places none |
| `--topics N` | options on offer, 2–8 (default 4) |
| `--noise N` | half-width of the error on a private evaluation (default 90) |
| `--seed N` | room generator seed (default 1) |
| `--specialists N` | `N` members each read one topic far more tightly than everybody else, and everybody else's read of that topic widens to match -- information is redistributed, not created |
| `--hidden-profile` | one decoy is planted above every member's own argmax except one member, who alone holds the fact that rules it out |
| `--cost-tiers` | with `--specialists`, a specialist's own turn costs ten times a lay member's, for the `cost/ep` column |
| `--blind-evidence` | a member's first turn, while the room is blind, is a deposit rather than a position (off by default) |
| `--directory` | fold the directory into the traced episode's own policy, so `--trace` can show a `knows` turn |
| `--defer-cap N` | turns a member may spend deferring to a topic's expert instead of arguing outside its own specialty (default 1, minimum 1); read by `hive+defer` and `hive+dir+defer` |
| `--context N` | rows each member's window holds; `0` (default) disables the window model entirely |
| `--rot F` | how hard the middle of that window is discounted, `0.0..=1.0` (default `0.0`) |
| `--context-sweep` | run the window ladder instead of comparing arms once |
| `--aside-cap N` | pairwise checks one member may open (default 1); under `hive+share` it caps distinct peers contacted instead; `0` makes every aside arm bit-identical to `hive+` |
| `--aside-cap N` | pairwise checks one member may open (default 1); under `hive+share` it caps distinct peers contacted instead; `0` makes every on-floor and alongside aside arm bit-identical to `hive+` |
| `--exchange-cap N` | private rows one member may write **off the floor** across an episode, read by `hive+rounds` (default 4); a separate knob because it bounds model calls rather than the room's turns; `0` makes `hive+rounds` bit-identical to `hive+` |
| `--history N` | prior episodes of `hive+` the `ladder+dir` arm earns its directory from (default 3) |
| `--budget N` `--quorum N` `--window N` | episode policy, overriding the tuned values |
| `--dominance N` `--repetition N` `--no-blind` | episode policy |
| `--scale-sweep` | sweep room size against channel topology instead of comparing arms once: seven channels, from nobody talking to everything shared, at every size in `--sizes`. Reports accuracy, floor rows and private contacts separately, because a channel that buys two points by writing four times the traffic has not obviously bought anything |
| `--sizes A,B,C` | the room sizes `--scale-sweep` walks (default `3,5,8,16,32,64`) |
| `--trace` | print one episode turn by turn |
| `--sweep` | score the policy grid, swept relative to the desk size |
| `--swarm` | run a federation of desks instead of one room |
| `--desks N` | channels in the federation, 2–64 (default 3) |
| `--per-desk N` | members on each channel, 2–256 (default 4) |
| `--bias N` | how much a desk overrates its own decoy (default 110) |
| `--agent-cmd CMD` | drive one episode through a real agent CLI |
| `--scenario PATH` | give the live room a real problem with private facts |
| `--repeat N` | run a live scenario N times and count both arms |
| `--json` | print one flat JSON object per arm, ahead of the tables |
| `--stats-check` | run the statistics module's self-check, and the check arms' own, and exit `0` or `1` |
| `calls/ep` (column) | model calls made in off-floor exchange rounds per episode — members *asked*, not rows written, so a declined round costs what it actually cost. Kept out of `cost/ep`, which is each speaker's own cost times its turns |
| `--timeout SECS` | per-turn deadline for a live agent or HTTP request (default 180) |
| `--api-base URL` | drive seats directly over HTTP instead of a CLI |
| `--api-key-env NAME` | env var carrying the HTTP backend's key (default `LADDER_API_KEY`) |
| `--model NAME` | the HTTP backend's default model (default `flash`) |
| `--wire openai\|anthropic` | which chat wire format the HTTP backend speaks (default `openai`) |
| `--model-cost model=N` | cost per 1000 tokens for `model`, for the usage table (repeatable) |
| `--seat-model agent_id=model` | per-seat model override for the HTTP backend (repeatable) |
| `--seat-cmd agent_id="command"` | per-seat command override for the CLI backend (repeatable) |
| `--specialist-model NAME` | model seated for a member the scenario marks `tier: reasoning` (`expert_on:` alone does not move a seat) |
| `--thinking on\|off` | whether the HTTP backend reasons before answering (default `on`) |

`--swarm --trace` prints the interleaved multi-channel transcript, and
`--swarm --noise` defaults to ±50 rather than ±90: at the single-room default
the desk bias is swamped, every desk is individually unbiased, and crossing a
channel would be measuring nothing. `--hidden-profile --noise` defaults to ±50
for the same reason and by the same rule — an explicit `--noise` still wins.

The two constants that shape the hidden profile are bounded on both sides, and
`sim/mod.rs` writes the arithmetic on each. `HIDDEN_LIFT` is `100`, so the planted
decoy reads **140** against the true option's **100**: at ±50 the difference of
two draws is triangular on ±100, so a lay member's own argmax is the decoy
`1 - (60/100)² / 2 ≈ 82%` of the time and the matched-budget poll scores 15%.
`GROUNDS_WEIGHT` is `45`, inside the window `(40, 65)`: *above* the decoy's
bare 40-point lead, so one grounded refutation flips a member that has seen it
and has yet to see anybody back anything; *below* `40 + 25`, the lead once one
peer is already behind the decoy, so a member reading the fact against a room
that has started backing the decoy needs the fact **plus** a peer that has
already crossed. Two signals carry where one does not, and both bounds are what
keep the profile solvable but not trivial.

## Layout

Every module that outgrew a single file is a directory: `mod.rs` holds its
module doc, its core types, and whatever re-exports the rest of the crate
actually needs; its siblings hold one cohesive slice of the rest. The `mod
sim;`-style declaration in `main.rs` is unchanged either way, since Rust
resolves it to `sim/mod.rs` transparently. [`LAYOUT.md`](LAYOUT.md) has the
full file-by-file table.
