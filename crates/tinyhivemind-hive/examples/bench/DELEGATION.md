# Delegation, measured

The delegation half of [the deliberation benchmark](README.md): what
`--specialists`, `--hidden-profile`, `--defer-cap`, `--history` and
`--cost-tiers` are for, and what they scored.

## Three questions

They exist to answer three questions about expert delegation. The
specification is [`docs/specs/expert-delegation.md`](../../../../docs/specs/expert-delegation.md);
the reading behind it is [`docs/research/delegation.md`](../../../../docs/research/delegation.md).
Its acceptance criteria were written before any of these numbers, and one of
them is that the mechanism must be able to lose and the loss must be
published.

**Q1 — does the floor reach the member who holds the fact?** `expert %` and
`to-expert` measure it: the share of episodes in which the room's decisive
member spoke at all, and the mean turn index at which it first did. `hive+dir`
adds `BidReason::Knows`, which is meant to move that number.

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

5000 rooms, `--specialists 2 --cost-tiers`:

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

**The directory changes nothing.** `hive+dir` and `hive+` are identical on
every shape measured — 82.1% on the uniform 5000-room bench, 74.2% with two
specialists, 0.4% on a hidden profile — down to the decision rate and the turn
count. `BidReason::Knows` fires for a member that is the directory's top
holder of the contested topic *and* has taken no position on it, and in this
simulation those two conditions never hold together: the only way to earn
directory weight without taking a position is a topiced `!evidence`, which
only a hidden-profile fact-holder ever emits, and only about an option that
has already carried and is therefore never the contested one. This is the
mechanism's own firing rule doing what it says, not a bug — but it means the
uniform-bench prediction of *zero* is confirmed for a stronger reason than the
one the spec gave, and that the shapes built to give it something to route on
did not.

**`!defer` pays for itself, barely.** `hive+defer` scores 74.3% against
`hive+`'s 74.2% with two specialists (+3.2 against the vote control where
`hive+` is +3.0), at 6.94 turns against 6.98 — it converts a turn spent
arguing outside a member's area into a turn somebody else spends inside
theirs, and finishes marginally sooner. The difference is well inside the
confidence interval. On a hidden profile it is neutral to slightly negative.

**A directed router is worse than a blind one.** `ladder+dir` routes to the
decisive member *more* often than `ladder` does (22.3% against 18.9%) and is
seven and a half points *less* accurate (45.1% against 52.6%). Directory
weight on a topic is earned by grounding it, so the heaviest holder is the
member who argued it hardest rather than the one who reads it best. Matching a
task against a description is the routing rule Claude Code's subagents and
CrewAI's role strings use, and on this benchmark it loses to a coin flip.
Per unit spent it is worse too: 95.8 right answers per thousand units against
115.4.

**On cost, delegation is a rounding error and uniform expense is a
catastrophe.** `hive+cost` buys 23.2 right answers per thousand units against
`all-reasoning`'s 10.6 — but the whole of that gap is that `all-reasoning`
puts every seat on the ten-unit tier for the same 74.2%. Nothing about the
delegation mechanism produced it; not spending ten units on seats that do not
need them did.

## The hidden profile is decided before delegation can act

`--hidden-profile` plants one decoy 150 points above the base quality of every
other decoy, so it reads 190 against the true option's 100, and at the ±50
noise the flag defaults to, every member but one answers the decoy. The poll
scores **0.0%**, which is the construction working: a matched-budget vote
cannot solve a hidden profile, by definition.

No arm solves it either. Every deliberating arm lands at 0.0–0.4%, and the
single-responder ladder's 21.2% is simply the chance that the one member drawn
happens to be the fact-holder — `ladder+dir` reaches 21.4%, inside the
interval. The reason is structural and worth stating
plainly, because it bounds what this shape can ever measure: a `!propose`
counts as a supporter, so four lay members each putting the same decoy on the
floor carry it *inside the blind round*, before anybody has read anybody. The
episode is in `Phase::Commit` by the time the fact-holder first sees a floor
to deposit its fact against, and the next turn records the decoy. The turn
budget is fifteen and the episodes end at 6.3.

The `GROUNDS_WEIGHT` and `HIDDEN_LIFT` constants in `sim.rs` are calibrated so
that, arithmetically, one deposited fact does not flip a mean lay member whose
whole desk still backs the decoy and one deposit plus one peer that has
already crossed does — the two-signal window `(90, 140)`, documented on the
constants themselves. That arithmetic is correct and the episode never reaches
it. `--hidden-profile --no-blind` does reach it — `hive+` scores 22.8% over 500
rooms there against 0.4% with the blind round on — which is the shape of the
finding: the blind opening round, worth 24 points of accuracy on the ordinary
bench, is what makes this particular hidden profile unsolvable.

`--history 1` and `--history 5` barely move it — 22.0% and 22.4% over 2000
rooms, against 21.9% for the undirected ladder on the same sample — for a
related reason: almost nobody ever grounds the true option, because the room
never argues it, so it carries no directory weight for a router to read and
`ladder+dir` mostly falls back to the same uninformed draw `ladder` makes.

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
