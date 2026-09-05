# Who knows, and what it is worth

**Date:** 2026-09-05
**Status:** Recorded
**Code:** `crates/tinyhivemind-hive/examples/bench` — `--specialists`,
`--hidden-profile`, `--blind-evidence`, `--defer-cap`, `--cost-tiers`, `--history`
**Spec:** [`docs/specs/expert-delegation.md`](../specs/expert-delegation.md)
**Decision:** [ADR 0007](../adr/0007-the-directory-is-folded-from-citations.md)

## What this asks

Three questions, written down before any of the numbers below existed, because
the spec's second acceptance criterion says the mechanism must be able to lose
and the loss must be published.

**Q1 — does the floor reach the member who holds the fact?** `fact %` is the
share of episodes in which the decisive member deposited a topiced `!evidence`
line *before the commit boundary*; `to-fact` is the mean turn index at which it
landed. A deposit arriving at or after the boundary is compute the room paid for
and could not use, and is scored as a miss.

**Q2 — does an informative router beat a uniform ladder?** `ladder` draws one
responder uniformly, the honest model of a router that knows nothing.
`ladder+dir` is handed a directory the room earned over prior episodes, renders
each candidate's lines into its `description`, names the topic the call turns
on, and picks the heaviest holder — the rule a subagent `description` and a role
string implement. `route %` says how often each picked the decisive member.

**Q3 — what does accuracy cost, once seats have prices?** `--cost-tiers`
charges a specialist's turn ten units against a lay member's one, and the cost
table reports `cost/ep` and `correct/kU`, right answers per thousand units.

A fourth measure, `rho`, is an obligation rather than a question: the spec's
fifth criterion requires it, and [What `rho` separates](#what-rho-separates) is
where it is answered.

## The simulated design

Five agents, four options, one genuinely best, seeded throughout. Every member
holds a private noisy evaluation of every option; the room's only route to the
answer is to pool what its members separately believe.

**Expertise.** Two shapes redistribute those evaluations without creating
information. `--specialists N` gives `N` members one topic each that they read
far more tightly than anybody else, and widens everybody else's read of it to
match: total information is constant, who holds it is not.

`--hidden-profile` plants one decoy `HIDDEN_LIFT` above the base quality of
every other option, so it reads **140** against the true option's **100**, and
gives exactly one member the grounded fact that rules it out. At the ±50 noise
the flag defaults to, the difference of two draws is triangular on ±100, so a
lay member's own argmax is the decoy `1 - (60/100)² / 2 ≈ 82%` of the time.
That is why the matched-budget poll scores 15%: a vote cannot solve a hidden
profile, by construction.

**Both constants are bounded on both sides**, and `sim.rs` carries the
arithmetic on each. `GROUNDS_WEIGHT` is `45`, inside the window `(40, 65)`:
above the decoy's bare 40-point lead, so one grounded refutation flips a member
that has yet to see anybody back anything; below `40 + 25`, the lead once one
peer is behind the decoy, so a member reading the fact against a room that has
started backing it needs the fact *plus* a peer that already crossed. Outside
that window the profile is trivial or unsolvable and measures nothing.

**`--blind-evidence`, and why it exists.** It changes the *participants* and
nothing in the library: while the room is `Visibility::Blind`, a member's first
turn deposits `!evidence #topic` — its own reading of the topic it knows best,
uncited because nothing is visible to cite — rather than putting an option on
the floor. Proposals begin at `Visibility::Full`.

It exists because of the federated finding. A `!propose` counts as a supporter,
so **a room whose members share a bias reaches quorum inside its own blind
round**: four lay members who privately favour the same decoy carry it in four
turns, the first non-blind turn is a commit turn, and a fact arriving then has
nothing left to change. The
[federated experiment](2026-09-02-federated-hidden-profile.md) reached it from
the other side — moving a desk's question to *before* it had backed anything was
the difference between failing outright and 77.5% — and the
[live rooms](2026-09-01-live-hidden-profile.md) recorded it: in every correct
episode of that run the five blind turns were five `!evidence` lines.

## The arms

`ladder` is one responder off the real `responder_plan`, answering alone in one
turn; `vote` is the matched-budget control, independent answers decided by
plurality on the *whole* budget. `hive+` is the tuned policy, `hive+dir` adds
`directory: Some(DirectoryPolicy::DEFAULT)` so `BidReason::Knows` is reachable,
`hive+defer` adds `defer_cap: Some(N)` with nothing routing the vacated turn,
and `hive+dir+defer` is both. `ladder+dir` is the ladder with the earned
directory's lines as each candidate's `description`, through the real
`accept_selection`. `hive+cost` and `all-reasoning` appear under `--cost-tiers`
only; `hive`, `hive+ref` and `hive+ev` are the P8 and P9 arms.

## Results

### The uniform room, which predicted zero

5000 rooms, ±90 noise, both openings. The spec's sixth criterion predicted zero
effect here: with homogeneous expertise there is nothing to route on.

```text
                      ordinary opening        --blind-evidence
5000 rooms          correct %      95% CI   correct %      95% CI  knows %
ladder                   57.6   56.2–59.0        57.6   56.2–59.0        —
vote                     78.5   77.4–79.6        78.5   77.4–79.6        —
hive+                    82.1   81.0–83.1        75.3   74.1–76.5      0.0
hive+dir                 82.1   81.0–83.1        75.7   74.5–76.8     75.1
hive+defer               82.1   81.0–83.1        75.3   74.1–76.5      0.0
hive+dir+defer           82.1   81.0–83.1        75.7   74.5–76.8     75.1
ladder+dir               49.5   48.1–50.9        98.8   98.5–99.1      0.0
```

Under the ordinary opening `hive+dir − vote` is `+3.6 [+2.8, +4.4]`, which is
`hive+ − vote` to the digit; the delegation arms match `hive+` down to the
decision rate and the turn count, and `knows %` is `0.0`: **`BidReason::Knows`
never fires at all.** It requires a member to be the directory's top holder of
the contested topic *and* to have taken no position on it, and where every
member's first move is a `!propose` those never hold together — the only way to
earn weight without a position is a topiced `!evidence`, and nobody emits one
until the argument is over. `ladder+dir` is **eight points worse than the
uninformed draw over the same five candidates**: Q2's answer here, and a loss.

The evidence-first opening makes `Knows` reachable — it wins the floor in three
episodes in four — and what it buys is `+0.4`, well inside the interval, for
**seven points of accuracy lost by the opening itself**: five of the fifteen
turns go on deposits nobody needed, turns rise from 6.75 to 11.49, and `hive+`
fails to decide 7% of the time rather than 0.6%. On an ordinary room it is a
cost, and off by default for that reason.

**`ladder+dir`'s 98.8% is an artifact and must not be read as a result.** The
arm tells its router which *topic* the call turns on, and in this benchmark that
topic is the correct option; with an evidence-first opening the directory records
who deposited a reading of `#truth`, and a member who deposited on `#truth`
usually favours `#truth`. The 92-point swing from the same arm's 49.5% is the
size of the leak, not of any mechanism. It is left in, unchanged and labelled,
because deleting an arm that started scoring well would be worse than explaining
why its score is not evidence.

### Two specialists

5000 rooms, `--specialists 2`, ±90. `fact %` is `93.4` and `to-fact` `2.0`
for every arm of the evidence-first column, and `—` for the other.

```text
                      ordinary opening        --blind-evidence
5000 rooms          correct %      95% CI   correct %      95% CI  knows %  route %
ladder                   52.6   51.2–54.0        52.6   51.2–54.0        —     18.9
vote                     71.1   69.9–72.4        71.1   69.9–72.4        —        —
hive+                    74.2   72.9–75.4        67.6   66.3–68.9      0.0        —
hive+dir                 74.2   72.9–75.4        68.0   66.7–69.2     78.3        —
hive+defer               74.3   73.1–75.5        67.2   65.9–68.5      0.0        —
hive+dir+defer           74.3   73.1–75.5        68.1   66.8–69.4     79.8        —
ladder+dir               45.1   43.8–46.5        84.4   83.4–85.4      0.0     22.3
```

`ladder+dir` routes to the decisive member **more** often than `ladder` does —
22.3% against 18.9% — and is seven and a half points *less* accurate. That is
the cleanest statement of Q2's answer in the experiment: directory weight on a
topic is earned by grounding it, so the heaviest holder is the member who argued
it hardest rather than the one who reads it best, and routing precisely to the
wrong criterion is worse than not routing. Its 84.4% is the leak above one step
less severe, and still a number read off a topic the arm was told. `hive+dir` is
`+0.4` over `hive+` and `hive+dir+defer` `+0.5`, both inside the interval.

### The price of a seat

The same rooms under `--cost-tiers`, charging a specialist ten units a turn and
everybody else one.

```text
arm              correct %   cost/ep    correct/kU
vote                  71.1     69.00         10.31
ladder                52.6      4.56        115.39
ladder+dir            84.4      3.24        260.33
hive+                 67.6     52.40         12.91
hive+cost             68.1     52.62         12.95
all-reasoning         67.6    115.27          5.87
```

Q3's answer is real and it is not the delegation mechanism's. `hive+cost` buys
12.95 right answers per thousand units against `all-reasoning`'s 5.87, and the
whole gap is that `all-reasoning` puts every seat on the ten-unit tier for the
*same* 67.6%. Nothing about `Knows` or `!defer` produced it; not spending ten
units on seats that do not need them did. `ladder+dir`'s 260 is the artifact
above, priced.

### The hidden profile, which is the shape it was built for

5000 rooms, `--hidden-profile`, ±50.

```text
                      ordinary opening                --blind-evidence
5000 rooms          correct %   fact %    rho   correct %      95% CI   fact %  knows %    rho
ladder                   35.1        —      —        35.1   33.7–36.4        —        —      —
vote                     15.0        —      —        15.0   14.1–16.1        —        —      —
hive+                    15.3      1.5   0.17        66.3   65.0–67.6     96.8      0.0   0.32
hive+dir                 15.3      1.5   0.17        65.8   64.5–67.1     95.7     77.5   0.37
hive+defer               15.4      1.4   0.17        66.8   65.4–68.1     96.8      0.0  -0.02
hive+dir+defer           15.4      1.4   0.17        66.6   65.3–67.9     95.7     77.3  -0.00
hive+ref                 15.3      1.8   0.32        53.3   51.9–54.7     96.4      0.0   0.31
hive+ev                  12.7      7.1   0.33        26.0   24.8–27.2     96.5      0.0   0.48
ladder+dir               34.6        —      —        64.1   62.8–65.4        —      0.0      —
```

Under the ordinary opening no deliberating arm solves it: every one lands
between 5.6% and 15.4% (`hive` alone, off-table, at 5.6%), `hive+ − vote` is
`+0.3 [+0.1, +0.5]`, and the ladder's 35.1% is the chance the one member drawn
is the fact-holder or a lay member whose noise fell the right way. **`fact %`
says why: the deciding fact reaches the floor in time in 1.5% of episodes.** The
room is in `Phase::Commit` by the time the fact-holder first sees a floor to
deposit against, and the next turn records the decoy. The arithmetic behind
`GROUNDS_WEIGHT` is correct and never reached. With the evidence-first opening
`fact %` goes to **96.8%** at a mean turn index of 2.1, the answer goes from 15%
to 66%, and `hive+ − vote` is `+51.3 [+49.8, +52.8]`.

That is the whole finding, and it is worth being exact about what it is a
finding *about*: same rooms, same private evaluations, same policy, same fold.
The one thing that changed is a participant policy that states what it knows
before what it wants. **The evidence-first opening is what makes any floor
mechanism reachable at all** — including the two P9 mechanisms, still losing but
now from 53.3% and 26.0% rather than from 15.3% and 12.7%.

And on the shape it was designed for, `hive+dir` **loses**: 65.8% against
`hive+`'s 66.3%, with `Knows` winning the floor in 77.5% of episodes.
`hive+dir+defer` is 66.6% against 66.8% for defer alone. `ladder+dir` at 64.1%
is the one place the directed router wins, and it wins by being told the topic.

So Q1's answer is yes, emphatically, and the opening buys it. Q2's is no:
**`hive+dir` does not beat `hive+` anywhere** — `+0.0` on the uniform room,
`+0.4` under the evidence-first opening, `+0.0` and `+0.4` with two specialists,
`−0.5` on the hidden profile, every one inside the interval.

### `!defer` is neutral

`hive+defer` is `+0.1` with two specialists, `+0.1` on the hidden profile under
the ordinary opening, `+0.5` under the evidence-first one and `−0.4` with two
specialists under it — a turn spent arguing outside a member's area converted
into one somebody else spends inside theirs, finishing marginally sooner, and
never outside the interval. Raising the cap to two over 2000 rooms moves
accuracy nothing (`hive+defer` 67.8%, `hive+dir+defer` 67.4% against `hive+`'s
66.6%, at 0.8 and 0.7 defers per episode) and moves `rho` to `-0.08` and
`-0.06` against `0.31`.

### What `rho` separates

The spec obliges this benchmark to print the rank correlation between a
member's final directory weight and its share of the episode's turns, and to
report the mechanism as having failed if it tracks speech even where accuracy
rose. `rho` is Pearson's correlation on tie-averaged ranks, computed in exact
integer arithmetic. Across the runs above it spans `-0.08` to `0.72`, and
separates the arms.

`hive+dir` under `--blind-evidence` on the uniform room reads the range's high
end, `0.72`, and every deliberating arm of that same blind-evidence uniform
room sits at `0.61`–`0.72`; `hive+dir` with two specialists and the
evidence-first opening reads `0.46`; `hive+` on the hidden profile under it
`0.32`; `hive+defer` on the same shape `-0.02`, and `-0.08` at
`--defer-cap 2`. `hive` on a hidden profile under the ordinary opening reads
`0.08` — the low end of the range, where the retired shortcut used to report a
spuriously high number for exactly this kind of tied, uninformative cell.

Neither of the two things doing the separating is the directory. **Depositing
before arguing** stops weight tracking turn count: an opening turn that states a
fact earns specialisation without earning a position, so the member who talks
most is no longer automatically the one who weighs most. **Deferring** does the
rest — a member that says "not mine" zeroes its own weight on that topic.

So the estimator *can* be made to stop measuring speech; and even at the
`0.61`–`0.72` the default bench's blind-evidence arms sit at, the directory's
weight tracks speech share only weakly at best — the accuracy numbers agree
that little of any result there could be credited to the directory having
found something.

### Not a seed artifact

The hidden-profile result with the evidence-first opening, at three seeds:

```text
--hidden-profile --blind-evidence   seed 1   seed 2   seed 3
vote                                  15.0     16.6     15.5
hive+                                 66.3     67.2     67.2
hive+dir                              65.8     66.3     67.1
ladder+dir                            64.1     64.6     64.5
```

The 51-point gap over the vote reproduces at all three, as does the ordering of
the delegation arms inside a point of each other.

## The live arm

The simulated participants are arithmetic: they do not misread a brief, coin a
second name for an option, or refuse to defer. The live arm is where those
become measurable, on scenarios rather than rooms of numbers.

`scenarios/index-lock-expert.txt` is the hidden profile written for a room with
a named specialist. The answer `#batch` is a conjunction of two halves that are
inert apart: `scout` holds a lock-escalation threshold with no transaction sizes
beside it, `dba` holds two batch sizes with no threshold beside them, and only
`planner` holds the fact that kills the decoy the brief plants. **Four designs
were tried and discarded before it, by measurement rather than by taste**, and
the header records each. The first leaked outright — polled alone, members
picked its truth **15 times out of 15 across three harnesses**, because the
option text stated the diagnosis. The next collapsed on a reasoning model
because one bulk writer on the option list made "a large transaction is
escalating" and "pause the nightly job" the same sentence; this design puts two
on the floor, so the threshold alone accuses both and the sizes alone accuse
neither.

Polled alone the scenario answers `#rollback` — three seats read the migration
as the cause, `planner` reads `#killqueries`, and only `dba` ever names the
truth. Plurality wrong in every poll of every row below, never more than one
member on it: the control being unable to win, which is what makes a room's
score mean anything.

### Scenario designs discarded by measurement

Design 5's failure is worth naming on its own. It shipped with a hole that live
rooms found in three harnesses out of three: `scout`'s fact said the threshold
could be restored by rebuilding the table, and every room read that as license
to converge on a coined `#rebuild`, `#rethreshold` or `#escalate` — a remedy
that was *correct* and simply not on the option list. The fact now says a
rebuild takes nine hours with writes blocked, and the task says nothing off the
list can ship. A room that converges on a coined topic is recorded as
undecided-wrong, so a live scenario must close every remedy it does not list,
not only the wrong ones.

### The live matrix

Ten rows, twenty-seven rounds, two hundred and sixty-six agent turns. Every row ran
both arms: the room, and the matched-budget poll of the same seats through the
same backend. `expert` is rounds in which the scenario's `truth_expert` spoke
before the commit, over rounds whose commit chain reaches something it said.
Tokens and cost print for the HTTP backend only, in the harness's unit — 1 per
1000 tokens times the model's price, so a `reasoning` round costs ten times a
`flash` round of the same length.

| row | backend | rounds | hive ✓/decided | poll ✓ | turns/ep | s/ep | tokens/ep | cost/ep | expert | `!defer` |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `checkout-503` | HTTP `flash` | 3 | **3 / 3** | 0 / 3 | 7.3 | 451 | 27,543 | 24.7 | — | 0 |
| `index-lock-expert` | HTTP `flash` | 3 | 1 / 2 | 0 / 3 | 11.7 | 842 | 48,483 | 46.0 | 3 / 2 | 0 |
| `index-lock-expert` | HTTP, `--specialist-model reasoning` | 3 | 0 / 3 | 0 / 3 | 7.0 | 322 | 19,999 | 197.7 | 3 / 1 | 0 |
| `index-lock-expert` | `claude -p --model flash` | 3 | **3 / 3** | 0 / 3 | 13.3 | 697 | — | — | 3 / 2 | 0 |
| `index-lock-expert` | `opencode run -m ladder/flash` | 3 | 1 / 2 | 0 / 3 | 12.3 | 374 | — | — | 3 / 1 | 0 |
| `index-lock-expert` | `codex exec` → `deepseek/deepseek-v4-flash` | 3 | 0 / 2 | 0 / 3 | 12.3 | 305 | — | — | 3 / 0 | 0 |
| `index-lock-tiers` | HTTP, `--specialist-model reasoning` | 3 | 0 / 3 | 0 / 3 | 6.0 | 318 | 16,865 | 166.0 | 3 / 1 | 0 |
| `index-lock-tiers` | HTTP, every seat `reasoning` | 2 | 0 / 2 | 0 / 2 | 7.5 | 489 | 20,858 | 207.0 | 2 / 1 | 0 |
| `checkout-503-federated` | HTTP `flash`, `--swarm` | 1 | 0 / 1 | 0 / 1 | 15 | 764 | 31,192 | 28.0 | — | 0 |
| `index-lock-tiers` | HTTP, four `flash` seats + `dba` on `reasoning` | 3 | 1 / 3 | 0 / 3 | 8.7 | 363 | 32,979 | 108.7 | 3 / 3 | 0 |

The last row is the corrected mixed-tier run, the harness defect described
below now fixed: only `dba` runs on `reasoning`, the other four seats stay on
`flash`. Round 1 committed `#batch` (correct) in 14 turns, 152 units, 48
cheap / 104 reasoning; rounds 2 and 3 both committed `#rollback` (wrong) in 6
turns each, at 108 and 66 units. `dba` spoke before every commit and was cited
by all three; it wrote 17–43% of a round's tokens for 54–90% of its cost, and
the one correct round was also the longest.

**The poll never found the answer, in any row** — twenty-seven rounds, wrong or
tied every time. On `index-lock-expert` and `index-lock-tiers` it returned
`#rollback` or `#archive`; on `checkout-503`, `#retries` three times in three,
the decoy the brief plants.

**The fact-holder spoke before the commit in every room that had one** — 23 of
23 rounds across eight rows, at turn 4 in eighteen of the first twenty. Q1's
live answer is yes, and the evidence-first opening buys it. **And fourteen of
those twenty-three rooms still got it wrong.** Reaching the holder is not the
bottleneck; weighing what it says is.

**`!defer` was never used.** Not one of the 266 turns, in any harness, on any
row, although `!defer #topic` sits in the move list every participant reads,
with its rules beside the markers they did use. The federated runs established
that a move *outside* the list is never used; this establishes something
narrower and less comfortable — a move inside it is not necessarily used either.
A model asked for one line still prefers a position on a topic it knows nothing
about to saying so, and the simulated arms that priced `!defer` were pricing a
move real participants did not make.

**The directory was in the prompt and never won a turn.** `directory_block`
renders into every deliberation prompt from the moment any topiced trace exists
— one `#topic: agent weight (spec N, cred N)` line per contested topic, holders
in descending weight — empty only on the blind first turn and absent from the
commit prompt. Across all 266 turns none was awarded on `BidReason::Knows`; the
reasons recorded are `salience`, `dissent`, `addressed` and `quiet`. Same
structural reason as the simulation: by the time a topic is contested, its top
holder has taken a position on it.

**The reasoning model on the expert seat did not help, and the rooms with it
lost faster.** Both `--specialist-model` rows converged on the decoy inside
seven turns — `index-lock-tiers` in six, `index-lock-expert` in seven — well
under the thirteen-plus turns the same scenario took on `flash`, because those
rooms had no deliberation phase at all: three of the five blind opening turns
were `!propose #rollback`, which is quorum, so the first non-blind turn was a
commit turn. What `dba` said in them is the finding. It never stated its
numbers — in all five rounds it spent its blind turn proposing `#rollback`,
"the archive and reconciliation jobs are unchanged from their year-long
pattern", turning the two batch sizes into an argument that they are *not* the
cause, exactly the non-event reading the scenario header predicts of a member
holding them. `scout`'s threshold was on the floor in the same blind round;
nobody held the two against each other, because by the time anyone could read
both the room had carried `#rollback`.

**`claude` scored 3 of 3 on a scenario the same model lost through three other
harnesses.** Same ladder, same `flash` behind it; what differs is the agent
around it, and the transcripts differ in three ways. The `claude` rooms are
longer — 13.3 turns and 697 s an episode against 7.0 and 322 s for the HTTP
specialist row — and spend the extra turns killing the decoy: `planner` deposits
the forward-only rollback fact and two members `!object` to `#rollback` on it,
so `#rollback` is dead before anything else is settled. They carry `#batch` on a
chain of `!support` lines citing prior turns by sequence, and commit on that
chain rather than restating a position. And — the honest caveat — they did not
do the arithmetic the scenario was built around: `scout` names the
reconciliation job in its *blind* turn, before any threshold or size is on the
floor, and the room converges on a guess that happens to be right. The room that
visibly performed the intended inference is an `opencode` one, where `dba` put
both commit sizes on the floor and used them to refute its own `#archive`
support — and that round **exhausted** at 15 turns without a commit.

**On cost, the live rows say what the simulation said, more bluntly.** The two
`flash` rows that decided anything cost 25 and 46 units an episode; the three
rows with a reasoning model cost 166, 198 and 207 and scored **0 correct in 8
rounds**. One caveat is load-bearing and was a harness defect, now fixed:
`is_specialist` was true for any seat with an `expert_on` line, and every seat
in both scenarios has one, so `--specialist-model reasoning` put the whole room
on `reasoning` rather than `dba` alone. **So this matrix did not test the
mixed-tier claim at all** — it compared flash-only rooms against all-reasoning
rooms, and the cheap rooms won.

Seating by tier fixed the defect (the corrected row above). That run scored 1
correct in 3 rounds at 108.7 units — better than either buggy all-reasoning
row, still costlier than `flash`-only without clearly beating it on accuracy.
One round is not a rate, but a specialist's presence alone did not fix a room
that never weighs what it hears — `dba` spoke before and was cited by every
commit, including the two it got wrong.

**The federation completed one round and decided wrongly.** Platform and Data
converged on `#retries`, Release on `#scale`, the plurality was `#retries`, and
the poll of the same nine returned `#retries` too — both wrong, and the truth
`#pool` was nobody's. Two messages crossed a channel and nothing was stranded.
What crossed was accurate and useless: `data-lead` asked `@#release` whether the
retry path was active, and Release answered — shipped disabled, zero retries
fired today, the release moved the client timeout 2s→10s — then said the same
on its own desk. `release-sdk` read it and supported `#scale`; `data-dba` read
it and committed `#retries`. That is run four of the federated experiment again
on a different model: the protocol moved the evidence and the rooms reasoned
past it.

### What the live arm does not show

Three rounds a row, one model family behind every seat, one scenario family, and
the federated row is a single episode. Nothing here is a rate: `claude`'s 3 of 3
and the specialist rows' 0 of 8 are a handful of episodes each, and the
difference between 1 of 3 and 3 of 3 here is one room's turn order. The
corrected mixed-tier row is a genuine measurement now — only `dba` on
`reasoning`, everyone else on `flash` — but it is one row of three rounds, not a
rate, and the one correct round was also the longest, which is its own caveat.
And every row's poll losing is a property of scenarios *selected* until it
did — what makes a room's score legible, not a result about polls.

## What the simulation does not show

**It does not show the directory is wrong, only that it is unnecessary here.**
Every shape measured gives the room enough turns that the fact-holder speaks
anyway once the opening lets it. A tighter budget, more members, or a genuinely
quiet fact-holder is where routing would have something to buy, and none of
those is in this sample.

**The topic handed to `ladder+dir` is the correct option's name**, which no real
router is handed, so every number that arm produced under `--blind-evidence` is
an upper bound rather than a score.

**`--history` went the wrong way.** Over 2000 rooms with two specialists,
`ladder+dir` scores 52.1% at `--history 0` — the null control, where every
description is `None` and the arm reproduces `ladder` — then 44.8, 44.0, 43.9
and 43.6 at one, three, five and ten prior episodes. The estimate is not
undertrained: it converges, slowly, on the wrong member.

**A citation ring is not detected**, and no simulated participant tries one. And
**nothing here is evidence about language models**: whether a real agent will
deposit before arguing, defer honestly, or route to a peer it has read is what
the live arm is for.

## What was done about it

Per the spec's acceptance criteria, under the discipline P9 established:

- **`EpisodePolicy::DEFAULT` keeps `directory: None` and `defer_cap: None`.**
  Criterion 4 ships off anything that helps hidden profiles at more than two
  points on the uniform bench; this costs nothing there and buys nothing on the
  hidden profile, which is a weaker case for defaulting it on, not a stronger.
- **The circularity number is published next to accuracy**, per criterion 5.
- **The code stays, opt-in and covered.** A host with a real hidden profile, a
  heterogeneous roster and a tighter budget starts from a mechanism and a number
  rather than an argument.
- **`--blind-evidence` stays off too**, at seven points on an ordinary room. It
  is a participant policy, not a library setting, and the honest form of the
  finding is "state facts before positions *when the room might share a bias*",
  which a host knows and the library does not.

## Reproducing

```sh
cargo build --release -p tinyhivemind-hive --example bench
B=target/release/examples/bench
$B --episodes 5000
$B --episodes 5000 --blind-evidence
$B --episodes 5000 --specialists 2
$B --episodes 5000 --specialists 2 --blind-evidence
$B --episodes 5000 --specialists 2 --blind-evidence --cost-tiers
$B --episodes 5000 --hidden-profile
$B --episodes 5000 --hidden-profile --blind-evidence
$B --episodes 2000 --hidden-profile --blind-evidence --defer-cap 2
$B --episodes 5000 --hidden-profile --blind-evidence --seed 2   # and --seed 3
```

Everything above is seeded and reproduces exactly. The live arm does not, and
one run of it is an anecdote.
