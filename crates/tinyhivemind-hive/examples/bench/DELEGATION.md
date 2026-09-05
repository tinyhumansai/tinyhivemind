# Delegation, measured

The delegation half of [the deliberation benchmark](README.md): what
`--specialists`, `--hidden-profile`, `--defer-cap`, `--history`,
`--cost-tiers` and `--blind-evidence` are for, and what they scored.

## Three questions

They exist to answer three questions about expert delegation. The write-up of
what they scored, at 5000 rooms per shape, is
[`docs/experiments/2026-09-05-expert-delegation.md`](../../../../docs/experiments/2026-09-05-expert-delegation.md).
The specification is [`docs/specs/expert-delegation.md`](../../../../docs/specs/expert-delegation.md);
the reading behind it is [`docs/research/delegation.md`](../../../../docs/research/delegation.md).
Its acceptance criteria were written before any of these numbers, and one of
them is that the mechanism must be able to lose and the loss must be
published.

**Q1 — does the floor reach the member who holds the fact?** `fact %` and
`to-fact` measure it: the share of episodes in which the room's decisive member
deposited its knowledge — a topiced `!evidence` line — *before the commit
boundary*, and the mean turn index at which it did. `knows %` reports how often
`BidReason::Knows`, which `hive+dir` adds, actually won the floor.

**Q2 — how precisely does a router route?** `route %` measures it, over the
two ladder arms. `ladder` draws a responder uniformly, which is the honest
model of a router that knows nothing about the task. `ladder+dir` is handed a
directory the room earned over `--history` prior episodes of `hive+` on the
same room, renders each member's own directory lines into that candidate's
`description`, tells the router which *topic* the call turns on — never which
option is right — and lets it pick the heaviest holder. Both go through the
real `responder_plan` and the real `accept_selection`.

**Q3 — what does accuracy cost?** `--cost-tiers` charges a specialist's turn
ten units against a lay member's one, and the cost table under `--cost-tiers`
prints `cost/ep` and `correct/kU` — right answers per thousand units spent —
for `hive+cost` (the delegating room, expensive seats only where the
specialists are) against `all-reasoning` (the same policy with every seat on
the expensive tier).

## What the arms actually scored

5000 rooms, `--specialists 2 --cost-tiers`, ordinary opening:

```text
arm              correct %   cost/ep    correct/kU
vote                  71.1     69.00         10.31
ladder                52.6      4.56        115.39
ladder+dir            45.1      4.71         95.81
hive+                 74.2     32.30         22.97
hive+cost             74.3     32.05         23.20
all-reasoning         74.2     69.80         10.63
```

Four findings, none of them the hoped-for one.

**The directory changes nothing until somebody deposits.** `hive+dir` and
`hive+` are identical on every shape run under the ordinary opening — 82.1% on
the uniform 5000-room bench, 74.2% with two specialists, 15.3% on a hidden
profile — down to the decision rate and the turn count, and `knows %` is
`0.0`. `BidReason::Knows` fires for a member that is the directory's top holder
of the contested topic *and* has taken no position on it, and under an opening
where every member's first move is a `!propose` those two conditions never hold
together: the only way to earn directory weight without taking a position is a
topiced `!evidence`, and nobody emits one until the argument is already over.

`--blind-evidence` is what makes the rule satisfiable, and it is a change to
the *participants* rather than to the library — see
[the README](README.md#the-evidence-first-opening). With it, every member opens
on a deposit, the contested topic has holders as soon as it exists, and `Knows`
wins the floor in **75–80% of episodes** on every shape. What it buys once it
fires is small and positive: `hive+dir` scores 68.0% against `hive+`'s 67.6%
with two specialists and 75.7% against 75.3% on a uniform room, both well
inside the interval; on a hidden profile it is 65.8% against 66.3%, which is a
loss. The mechanism is reachable, it is not free, and on the shape it was
designed for it does not yet pay.

**`!defer` pays for itself, barely.** `hive+defer` scores 74.3% against
`hive+`'s 74.2% with two specialists (+3.2 against the vote control where
`hive+` is +3.0), at 6.94 turns against 6.98 — it converts a turn spent
arguing outside a member's area into a turn somebody else spends inside
theirs, and finishes marginally sooner. The difference is well inside the
confidence interval. On a hidden profile it is neutral to slightly negative.
Under `--blind-evidence` it is the same story with the sign occasionally
flipped: 67.2% against 67.6% with two specialists, 66.8% against 66.3% on a
hidden profile.

**A directed router is worse than a blind one — until members deposit, and
then it is suspiciously better.** Under the ordinary opening `ladder+dir`
routes to the decisive member *more* often than `ladder` does (22.3% against
18.9%) and is seven and a half points *less* accurate (45.1% against 52.6%).
Directory weight on a topic is earned by grounding it, so the heaviest holder
is the member who argued it hardest rather than the one who reads it best.
Matching a task against a description is the routing rule Claude Code's
subagents and CrewAI's role strings use, and on this benchmark it loses to a
uniform draw over the same five candidates. Per unit spent it is worse too:
95.8 right answers per thousand units against 115.4.

Under `--blind-evidence` the same arm scores 84.4% with two specialists and
98.8% on a uniform room, and **neither number should be read as a win**. The
arm tells its router which topic the call turns on, and in this benchmark that
topic is the correct option; with an evidence-first opening the directory
records who deposited a reading of it, and a member who deposited on `#truth`
usually favours `#truth`. The router is reading a vote off a description that
was never meant to carry one. It is left in, unchanged and labelled, because
deleting an arm that started scoring well would be worse than explaining why
its score is not evidence.

**On cost, delegation is a rounding error and uniform expense is a
catastrophe.** `hive+cost` buys 23.2 right answers per thousand units against
`all-reasoning`'s 10.6 — but the whole of that gap is that `all-reasoning`
puts every seat on the ten-unit tier for the same 74.2%. Nothing about the
delegation mechanism produced it; not spending ten units on seats that do not
need them did.

With `--blind-evidence` the same table reads:

```text
arm              correct %   cost/ep    correct/kU
vote                  71.1     69.00         10.31
ladder                52.6      4.56        115.39
ladder+dir            84.4      3.24        260.33
hive+                 67.6     52.40         12.91
hive+cost             68.1     52.62         12.95
all-reasoning         67.6    115.27          5.87
```

Every deliberating arm costs more, because five turns now go on deposits and
the specialists among them are the expensive seats. `ladder+dir`'s 260 per
thousand units is the routing artifact above, priced.

## The hidden profile, before and after the opening

`--hidden-profile` plants one decoy `HIDDEN_LIFT` above the base quality of
every other option, so it reads 140 against the true option's 100, and at the
±50 noise the flag defaults to, a lay member's own argmax is the decoy about
82% of the time. The matched-budget poll scores **15.0%**, which is the
construction working: a vote cannot solve a hidden profile, by definition.

Under the ordinary opening, no deliberating arm solves it either. Every one
lands at 5.6–15.4%, and the single-responder ladder's 35.1% is simply the
chance that the one member drawn is the fact-holder or a lay member whose noise
fell the right way. The reason is structural: a `!propose` counts as a
supporter, so four lay members each putting the same decoy on the floor carry
it *inside the blind round*, before anybody has read anybody. The episode is in
`Phase::Commit` by the time the fact-holder first sees a floor to deposit
against, and the next turn records the decoy. `fact %` says so directly — the
deciding fact reaches the floor in time in **1.5%** of episodes.

Under `--blind-evidence` it reaches the floor in **96.8%** of them, at a mean
turn index of 2.1, and the arms separate:

```text
5000 rooms, --hidden-profile        correct %   fact %   knows %      rho
vote                                     15.0        —         —        —
ladder                                   35.1        —         —        —
ladder+dir                               64.1        —         —        —
hive                                     67.0     95.4       0.0     0.29
hive+                                    66.3     96.8       0.0     0.32
hive+dir                                 65.8     95.7      77.5     0.37
hive+defer                               66.8     96.8       0.0    -0.02
hive+dir+defer                           66.6     95.7      77.3    -0.00
hive+ref                                 53.3     96.4       0.0     0.31
hive+ev                                  26.0     96.5       0.0     0.48
```

`hive+ − vote` is `+51.3 [+49.8, +52.8]`. That is the whole finding, and it is
a finding about *when a member speaks*, not about any mechanism in the library:
the same rooms, the same private evaluations, the same policy, the same fold,
and a participant policy that states what it knows before it states what it
wants.

The two constants are calibrated so that one grounded refutation, once seen, is
enough to put the truth ahead for a member that has yet to see anybody back
anything, and not enough once one peer is already behind the decoy — the window
`(40, 65)`, with `GROUNDS_WEIGHT` at 45, documented on the constants
themselves. Under the ordinary opening that arithmetic is correct and the
episode never reaches it.

`--defer-cap 2` over 2000 rooms moves nothing outside the interval:
`hive+defer` 67.8% and `hive+dir+defer` 67.4% against `hive+`'s 66.6%, at
0.8 and 0.7 defers per episode. What it does move is `rho`, to `-0.08` and
`-0.06` against `0.31` — a member that says "not mine" zeroes its own
directory weight on that topic, and doing it twice per episode is enough to
pull the estimate below zero.

## How much history is worth

Less than none. Over 2000 rooms with two specialists:

```text
--history        0     1     3     5    10
ladder+dir    52.1  44.8  44.0  43.9  43.6
```

`--history 0` is the null control: an empty directory, so every candidate's
description is `None`, the router falls back to the uninformed draw, and the
arm scores exactly what `ladder` scores on the same sample (52.1%). One
episode of history costs seven points, and every episode after that costs a
little more. The estimate is not undertrained and does not sharpen with shared
history — it converges, slowly, on the wrong member. A host planning to
accumulate a directory across episodes should read that as the finding it is.

## The live matrix

Ten backend rows, twenty-seven rounds, two hundred and sixty-six agent turns, on
`index-lock-expert`, `index-lock-tiers`, `checkout-503` and the federated
`checkout-503-federated`. `expert` is the rounds in which the scenario's
`truth_expert` spoke before the commit, over the rounds whose winning commit
chain reaches something it said; cost is the harness's own unit and prints for
the HTTP backend only.

| row | backend | rounds | hive ✓/decided | poll ✓ | turns/ep | s/ep | cost/ep | expert |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `checkout-503` | HTTP `flash` | 3 | 3 / 3 | 0 / 3 | 7.3 | 451 | 24.7 | — |
| `index-lock-expert` | HTTP `flash` | 3 | 1 / 2 | 0 / 3 | 11.7 | 842 | 46.0 | 3 / 2 |
| `index-lock-expert` | HTTP, `--specialist-model reasoning` | 3 | 0 / 3 | 0 / 3 | 7.0 | 322 | 197.7 | 3 / 1 |
| `index-lock-expert` | `claude -p --model flash` | 3 | 3 / 3 | 0 / 3 | 13.3 | 697 | — | 3 / 2 |
| `index-lock-expert` | `opencode run -m ladder/flash` | 3 | 1 / 2 | 0 / 3 | 12.3 | 374 | — | 3 / 1 |
| `index-lock-expert` | `codex exec` → `deepseek/deepseek-v4-flash` | 3 | 0 / 2 | 0 / 3 | 12.3 | 305 | — | 3 / 0 |
| `index-lock-tiers` | HTTP, `--specialist-model reasoning` | 3 | 0 / 3 | 0 / 3 | 6.0 | 318 | 166.0 | 3 / 1 |
| `index-lock-tiers` | HTTP, every seat `reasoning` | 2 | 0 / 2 | 0 / 2 | 7.5 | 489 | 207.0 | 2 / 1 |
| `checkout-503-federated` | HTTP `flash`, `--swarm` | 1 | 0 / 1 | 0 / 1 | 15 | 764 | 28.0 | — |
| `index-lock-tiers` | HTTP, four `flash` seats + `dba` on `reasoning` | 3 | 1 / 3 | 0 / 3 | 8.7 | 363 | 108.7 | 3 / 3 |

Five things the transcripts say, and one harness defect they exposed — now
fixed, with a corrected row measuring what the defect had prevented.

The **poll found the answer in none of the twenty-seven rounds**, which is the
control being unable to win — the property the scenarios were rewritten four
times to get.

The **fact-holder spoke before the commit in all twenty-three rounds that had
one**, at turn 4 in eighteen of the first twenty, and **fourteen of those
rooms were still wrong**. Reaching the holder is not the bottleneck.

**`!defer` was used on none of the 266 turns** and **no turn was awarded on
`BidReason::Knows`**, although the move was in every move list and the directory
block was in every deliberation prompt.

**Putting a reasoning model on the expert seat made the room worse and faster,
in the two buggy rows that put the whole room on it.** Both converged on the
decoy inside seven turns — `index-lock-tiers` in six, `index-lock-expert` in
seven — because three of the five blind opening turns were the same
`!propose`, which is already quorum, so the first non-blind turn was a commit
turn. `dba` never stated its numbers in those rooms — it spent its blind turn
arguing that neither batch size has moved in a year and therefore neither is
the cause.

**The harness defect, now fixed:** `is_specialist` was true for any seat the
scenario gave an `expert_on` line, and every seat in these scenarios has one,
so `--specialist-model reasoning` put the whole room on the expensive model
rather than `dba` alone; the per-tier usage lines confirmed it by charging the
`cheap` tier at the reasoning price. Those two rows therefore compared
flash-only rooms against all-reasoning rooms — where the cheap rooms won, 0
correct in 8 rounds at 166–207 units an episode against 25–46 — and said
nothing about mixed tiers. Seating the specialist model by tier fixed this;
the corrected `index-lock-tiers` row above, with only `dba` on `reasoning`,
scored 1 correct in 3 rounds at 108.7 units an episode — better than either
buggy all-reasoning row and still costlier than the `flash`-only rows, without
clearly beating them on accuracy.

Three rounds a row, one model family, one scenario family, and one federated
episode. Nothing here is a rate; the full write-up is in
[`docs/experiments/2026-09-05-expert-delegation.md`](../../../../docs/experiments/2026-09-05-expert-delegation.md).
