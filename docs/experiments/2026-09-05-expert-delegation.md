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
hive+                    15.3      1.5   0.74        66.3   65.0–67.6     96.8      0.0   0.37
hive+dir                 15.3      1.5   0.74        65.8   64.5–67.1     95.7     77.5   0.42
hive+defer               15.4      1.4   0.74        66.8   65.4–68.1     96.8      0.0   0.07
hive+dir+defer           15.4      1.4   0.74        66.6   65.3–67.9     95.7     77.3   0.10
hive+ref                 15.3      1.8   0.73        53.3   51.9–54.7     96.4      0.0   0.37
hive+ev                  12.7      7.1   0.74        26.0   24.8–27.2     96.5      0.0   0.54
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
66.6%, at 0.8 and 0.7 defers per episode) and moves `rho` to `0.06` and `0.08`
against `0.36`.

### What `rho` separates

The spec obliges this benchmark to print the Spearman rank correlation between a
member's final directory weight and its share of the episode's turns, and to
report the mechanism as having failed if it tracks speech even where accuracy
rose. Across the runs above it spans `0.06` to `0.85`, and separates the arms.

`hive` on a hidden profile under the ordinary opening reads `0.83`; every
deliberating arm of the uniform room `0.72`–`0.75`; `hive+dir` with two
specialists and the evidence-first opening `0.50`; `hive+` on the hidden profile
under it `0.37`; `hive+defer` on the same shape `0.07`, and `0.06` at
`--defer-cap 2`.

Neither of the two things doing the separating is the directory. **Depositing
before arguing** stops weight tracking turn count: an opening turn that states a
fact earns specialisation without earning a position, so the member who talks
most is no longer automatically the one who weighs most. **Deferring** does the
rest — a member that says "not mine" zeroes its own weight on that topic.

So the estimator *can* be made to stop measuring speech; and at `0.72`, where
every arm of the default bench sits, very little of any result there could be
credited to the directory having found something. It did not, and the accuracy
numbers agree.

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
become measurable, and it runs on scenarios rather than rooms of numbers.

`scenarios/index-lock-expert.txt` is the hidden profile written for a room with
a named specialist. The answer `#batch` is a conjunction of two halves that are
inert apart: `scout` holds a lock-escalation threshold with no transaction sizes
beside it, `dba` holds two batch sizes with no threshold beside them, and only
`planner` holds the fact that kills the decoy the brief plants. **Four designs
were tried and discarded before it, by measurement rather than by taste**, and
the header records each. The first index-lock design leaked outright: polled
alone, members picked its truth **15 times out of 15 across three harnesses**,
because the option text stated the diagnosis and every private fact only
eliminated, so the model's prior filled the gap. The next collapsed on a
reasoning model because one bulk writer on the option list made "a large
transaction is escalating" and "pause the nightly job" the same sentence. This
design puts two on the floor, so the threshold alone accuses both and the sizes
alone accuse neither.

Polled alone the scenario answers `#rollback` — three runs, `flash` twice and a
reasoning model once:

| member | run 1 (`flash`) | run 2 (`flash`) | run 3 (reasoning) |
| --- | --- | --- | --- |
| `planner` | `#killqueries` | `#killqueries` | `#killqueries` |
| `sre` | `#rollback` | `#rollback` | `#rollback` |
| `analyst` | `#rollback` | `#rollback` | `#rollback` |
| `scout` | `#rollback` | `#rollback` | `#rollback` |
| `dba` | `#batch` | `#archive` | `#batch` |
| **plurality** | `#rollback` | `#rollback` | `#rollback` |

Plurality wrong every time, never more than one member on the truth: the control
being unable to win, which is what makes a deliberating arm's score mean
anything.

<!-- LIVE RESULTS: filled in after the matrix completes -->

The matrix that is running, and that this section will carry:

| scenario | backend | repeats |
| --- | --- | --- |
| `checkout-503` | HTTP, `flash` | 5 |
| `index-lock-expert` | HTTP, `flash` | 5 |
| `index-lock-expert` | HTTP, `--specialist-model reasoning` | 5 |
| `index-lock-expert` | `claude -p --model flash` | 3 |
| `index-lock-expert` | `opencode run -m ladder/flash` | 3 |
| `index-lock-expert` | `codex exec` against OpenRouter `deepseek/deepseek-v4-flash` | 3 |
| `index-lock-tiers` | HTTP, mixed tiers | 5 |
| `index-lock-tiers` | HTTP, all-reasoning | 3 |
| `checkout-503-federated` | HTTP, `flash` | 3 |

`codex exec` is a CLI row by necessity: the router cannot relay a streaming
Responses request, so it cannot go through the harness's own HTTP backend.

`index-lock-tiers` is the same scenario with a `tier:` on every seat — four
`cheap` seats whose job is a lookup and a negation, one `reasoning` seat on
`dba`, the only member asked to hold a number against somebody else's threshold.
It prices that routing rather than changing the answer: the live form of Q3.

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
