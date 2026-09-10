# The deliberation benchmark

A simulation harness for `tinyhivemind-hive`. It runs whole deliberation episodes
against reproducible synthetic rooms and answers one question — **what does
each way of running a room cost, and what does it buy** — across four axes and
in six columns that mean the same thing for every arm.

```sh
# The grid: one table shape, four axes, six columns.
cargo run --release -p tinyhivemind-hive --example bench -- --grid
cargo run --release -p tinyhivemind-hive --example bench -- \
  --grid --topic all --scale 5,11,23 --complexity 1,3,5 --concurrency 1,2,4

# The mechanism probes: every arm, at one point of the grid.
cargo run --release -p tinyhivemind-hive --example bench

# Pin the cost model to the endpoint you actually deploy against.
cargo run --release -p tinyhivemind-hive --example bench -- \
  --calibrate --api-base "$URL" --model flash
```

## The six columns

Every arm, in every cell, reports the same six things, and each of them is a
quantity somebody pays or receives:

| column | what it is |
| --- | --- |
| **quality** | Share of episodes that decided the genuinely best option. |
| **speed** | Mean wall clock from brief to decision — a sum over *rounds*, since a round's turns run concurrently. |
| **thru** | Episodes one seat pool finishes per hour at that latency. |
| **conc** | Mean turns in flight. `1.0` is a strictly sequential protocol; the ceiling is the room's size. |
| **tok/ep** | Prompt and completion tokens one episode spends. |
| **tok/s** | The rate those tokens are drawn at — what a provider rate limit is written against. |

Five of the six come from a **model** of what a turn costs, documented with its
assumptions in [`COST.md`](COST.md). Every run prints the point it was taken
at, every constant has a flag, and `--calibrate` measures all four against a
real endpoint. Nothing here should be believed at one setting of those
constants that does not survive moving them.

The columns this replaced — `ns/step` and `episodes/s` — measured the *library*
rather than the system. They still exist, under a heading that says so, because
they answer a real question (what does the state machine cost, with every
agent's own time excluded — the answer is microseconds). They stopped sharing a
table with token and latency columns because `vote`'s `ns/step 0` and
`episodes/s inf` read as "this arm is free" rather than "this arm never calls
the library", which is what they mean.

## The grid

Four axes, walked as a cross product, with the same arms and the same columns
in every cell:

| axis | flag | points |
| --- | --- | --- |
| **topic** | `--topic` | `uniform` — everybody equally informed, pooling is the whole job. `expert` — some members hold some options far more tightly; routing is worth something. `hidden` — one member alone holds the fact that rules out the decoy everybody else prefers. `all` takes every point. |
| **scale** | `--scale` | Members per room. Any size from 2 up. |
| **complexity** | `--complexity` | `1`–`5`, or `all`. One ordinal knob standing for options-on-offer and evaluation noise together: level 1 is 2 options at ±40, level 3 is the harness's historical default of 4 at ±90, level 5 is 8 at ±150. |
| **concurrency** | `--concurrency` | Turns one round may authorize at once. `1` is the sequential episode and reproduces every recorded number bit-for-bit. |

A bare `--grid` is a **single cell** reproducing the single-room comparison,
and each axis is widened by naming it. That default is deliberate: a benchmark
whose cheapest invocation is a sixty-cell run is one nobody runs before
pushing, and a benchmark nobody runs stops being true.

Naming any axis selects the grid, so `--topic hidden` alone does what it says.
A value that is not a point on its axis stops the run rather than being skipped
— a misspelled topic quietly dropped would run a *smaller* grid than was asked
for and print it under the heading that was typed.

Each cell seeds its own rooms from its own coordinates, so two cells are
different rooms rather than the same rooms under different labels, and
re-running one cell alone draws exactly the rooms it had inside the whole grid.
A cell's label (`topic=hidden scale=9 complexity=4 concurrency=2`) is therefore
a complete reproduction recipe.

### What the grid runs

Five systems, not the twenty-four below: `ladder`, `vote`, `hive+`,
`hive+wide`, and `hive+pooled` as the unreachable ceiling. The twenty-four are
*mechanism probes* — each asks whether one move is worth its turns against a
control differing from it in exactly one thing — and reproducing that table in
every cell would print hundreds of rows to answer a question about four axes.
Run the plain `bench` for those; they answer a different question, and they
answer it at one point.

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
| `ladder` | Today's behaviour: `responder_plan` in `tinyhivemind` selects one responder off the real ladder and that agent answers alone. One deliberation turn — but two model calls when the ladder takes the `Select` rung, since the router has to be asked who answers before anybody does, and the two are sequential. |
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
| `hive+wide` / `hive+blind` | The tuned policy run in concurrent rounds — every round, then only while the room is blind. What concurrency costs, and which half of it is free. See [`DEPTH.md`](DEPTH.md). |
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

The simulated participants are mechanical, which is the point: a language model
would make the numbers unreproducible and confound protocol quality with model
quality. On its turn a participant, seeing exactly what `project_for` allowed:

1. **breaks a deadlock** — if two options both carry, it objects to a message
   advocating the one it rates lower. Adding support cannot resolve that state,
   because both stay above the threshold however much weight one gains;
   silencing an advocate can, which is why an objection names a *message*;
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

5000 rooms, 5 agents, 4 options, `--noise 90`, at the default cost model:

```text
arm                quality     speed       thru    conc    tok/ep     tok/s
ladder               57.6%     5.1s        701     1.0      1.6k       320
vote                 78.5%     2.6s       1.4k    15.0     12.3k      4.8k
hive                 73.3%    15.8s        228     1.0      5.3k       336
hive+                82.1%    17.3s        208     1.0      6.0k       345
hive+ref             75.0%    23.1s        156     1.0      8.7k       375
hive+ev              55.9%    26.4s        136     1.0     10.4k       393
hive+dir             82.1%    17.3s        208     1.0      6.0k       345
hive+defer           82.1%    17.3s        208     1.0      6.0k       345
hive+dir+defer       82.1%    17.3s        208     1.0      6.0k       345
ladder+dir           49.5%     5.1s        701     1.0      1.6k       320
hive+rounds          82.3%    34.3s        105     3.0     39.8k      1.2k
hive+pooled          91.5%    15.6s        231     1.0      5.2k       335
hive+wide            77.1%     8.7s        415     2.3      6.9k       798
hive+blind           82.1%     9.6s        374     1.8      6.0k       621
```

The tuned deliberation beats the matched-budget control by 3.6 points at half
the budget, and one responder off the ladder reaches 57.6% — for two model
calls, not one, because the ladder has to ask a router who should answer
before anybody does. The quorum
threshold and the turn budget decide that, and the blind round is worth 24
points on its own.

**And it takes six and a half times as long.** `vote` answers in one round;
`hive+` takes seven. Those 3.6 points cost 14.7 extra seconds — and save half
the tokens, because a poll pays a full base prompt for every member of the
room. That trade is the result, and it was unreadable for as long as the table
reported the library's nanoseconds instead of the system's seconds.

Two more things fall straight out of the six columns:

- **Concurrency buys most of the latency back.** `hive+wide` gives up 5 points
  and halves the wall clock; `hive+blind`, which widens only the blind round,
  gives up *nothing* and still saves 45% — see [`DEPTH.md`](DEPTH.md).
- **`hive+rounds` costs 6.6× the tokens of `hive+` for 0.2 points.** An
  off-floor exchange spends model calls rather than turns, so it never showed
  up in a table that counted turns. Now it does.

The three delegation arms score exactly what `hive+` scores, as the
specification predicted for a room of uniform expertise: with nothing to route
on, a directory routes nowhere. `ladder+dir` is eight points *worse* than the
uninformed ladder; [`DELEGATION.md`](DELEGATION.md) says why.

The two refutation arms lose, which is why both knobs are off in
`QuorumPolicy::DEFAULT`. `hive+ref` falls below even the vote control, and
`hive+ev` starves the room — it fails to decide two episodes in five.

The library's own cost is printed under a heading of its own, and is about
2.3 µs per step: six orders of magnitude below a model turn.
[The benchmark write-up](https://github.com/tinyhumansai/tinyhivemind/wiki/Benchmarks)
has the tables behind each of those, across desk sizes, plus what the benchmark
does not show.

## A horizon, and a spread

`--stages` runs a **chain** of decisions rather than one, and adds the control
the library's claim is about — one agent working a long task, compacting as it
goes. A room beats an *evicting* soloist once the window is tight and never a
*summarising* one: [`HORIZON.md`](HORIZON.md). `--facets` runs a task that is
several questions **at once** instead, one owner each, and there the room wins —
flat accuracy in width where the soloist decays: [`VARIETY.md`](VARIETY.md).

## Statistics

The second table printed under the first — `correct %`, its confidence
interval, `fact %`, `to-fact`, `knows %`, `defers/ep`, `route %`, `cost/ep`
and `rho` — is documented in [`STATISTICS.md`](STATISTICS.md), together with
the paired bootstrap and what its intervals do and do not license.

## The evidence-first opening

`--blind-evidence` makes a member's first turn, while the room is still blind,
a deposit rather than a position. See [`EVIDENCE.md`](EVIDENCE.md).

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

A federation of desks that can only reach each other by a referral: what a
crossing costs, how a member decides to make one, and what a live federation
prints. See [`SWARM.md`](SWARM.md).

## The context budget

Every arm above moves information for free, which is a misleading *engineering*
ceiling once a participant is a model with a bounded window. `--context-sweep`
charges for it across a ladder of capacities that evicts from the middle and
discounts survivors by a U-curve. `hive+pooled` loses a third of its lead once
its window matches its own payload; `hive+` barely notices, because it barely
uses the window. No squeeze makes the deliberating room the better choice on
this task, so the honest reading is narrower than "context economy vindicates
deliberation": fix the protocol first. [`CONTEXT.md`](CONTEXT.md) has the
numbers and why the sweep reports an ordering rather than a value.

## Flags

### The grid, and what a turn costs

| flag | meaning |
| --- | --- |
| `--grid` | walk the cross product of the four axes; a bare `--grid` is one cell |
| `--topic LIST` | `uniform`, `expert`, `hidden`, or `all` |
| `--scale LIST` | members per room, two or more |
| `--complexity LIST` | `1`–`5`, or `all` |
| `--concurrency LIST` | turns one round may authorize at once, one or more |
| `--tokens-per-row N` | prompt tokens one transcript row contributes (default 42) |
| `--tokens-per-turn N` | completion tokens one turn writes (default 180) |
| `--prompt-base N` | prompt tokens charged before the first row (default 600) |
| `--ttft N` | milliseconds to a seat's first completion token (default 450) |
| `--decode-rate N` | completion tokens a seat decodes per second (default 85) |
| `--calibrate` | measure those four against `--api-base` and print the flags that pin them |

Naming any axis selects `--grid`. A value off its axis, or a cost constant that
will not parse, stops the run — see [`COST.md`](COST.md) for why a silently
defaulted cost constant is worse than a stopped run.

### Everything else

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
| `--stages N` | run the chain ladder: N sub-decisions in sequence on one accumulating window; a list sweeps a ladder, a bare number runs one length |
| `--fidelity F` | what a summarised row is worth under `solo+fold`, `0.0..=1.0` (default `0.35`) |
| `--facets N` | run the variety ladder: N independent sub-decisions belonging to one task, scored only when every one is right; a list sweeps a ladder, a bare number runs one width |
| `--roles` | give each facet an owner (`facet % members`) that reads it at the room's base noise while everybody else widens; off by default, so the baseline measures the division of labour alone |
| `--round-width N` | turns one round may authorize concurrently, read by `hive+wide` and `hive+blind` (default 4); `0` makes both bit-identical to `hive+`. Every baseline arm -- everything but `hive+wide` and `hive+blind` -- runs at width one regardless of this flag, so no recorded number for those arms moves with it |
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
| `--desks N` `--per-desk N` | channels in the federation, 2–64 (default 3); members on each channel, 2–256 (default 4) |
| `--bias N` | how much a desk overrates its own decoy (default 110) |
| `--evidence` `--fact-noise N` | plant disqualifying facts, each on a desk other than the one that needs it, and carry them alongside readings; `--fact-noise N` also names `N` per mille of them wrong (0–1000), naming the truth instead of the decoy — see [`SCALE.md`](SCALE.md#--evidence-what-a-channel-carries-not-how-wide-it-is) |
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
module doc, its core types and the re-exports the rest of the crate needs, and
its siblings hold one cohesive slice each. A `mod sim;` in `main.rs` resolves to
`sim/mod.rs` transparently, so the split is invisible from outside.
[`LAYOUT.md`](LAYOUT.md) has the full file-by-file table.
