# Delegation, measured

The delegation half of [the deliberation benchmark](README.md): what
`--specialists`, `--hidden-profile`, `--defer-cap`, `--history`,
`--cost-tiers` and `--blind-evidence` are for, and what they scored.

## Three questions

They exist to answer three questions about expert delegation. The
specification is [`docs/specs/expert-delegation.md`](../../../../docs/specs/expert-delegation.md);
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
hive                                     67.0     95.4       0.0     0.36
hive+                                    66.3     96.8       0.0     0.37
hive+dir                                 65.8     95.7      77.5     0.42
hive+defer                               66.8     96.8       0.0     0.07
hive+dir+defer                           66.6     95.7      77.3     0.10
hive+ref                                 53.3     96.4       0.0     0.37
hive+ev                                  26.0     96.5       0.0     0.54
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
0.8 and 0.7 defers per episode. What it does move is `rho`, to 0.06 and 0.08
against 0.36 — a member that says "not mine" zeroes its own directory weight on
that topic, and doing it twice per episode is enough to pull the estimate off
the speaking order entirely.

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
