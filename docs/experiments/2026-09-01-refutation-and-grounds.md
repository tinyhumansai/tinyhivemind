# Refutation and evidential grounds, scored

Date: 2026-09-01. Branch: `shared-medium`.

[`docs/specs/refutation-and-grounds.md`](../specs/refutation-and-grounds.md)
ends with an acceptance criterion that says the benchmark arm must be able to
lose. It lost. This is the record.

## What was built

Two mechanisms, both pure folds, both specified before they were written:

- **`!refute #topic ^N`** — [ADR 0003](../adr/0003-refutation-links-evidence-to-a-topic.md).
  A cited fact argued against a hypothesis rather than against a person.
  `refutation_cap` distinct grounded refuters cap a topic out of contention.
- **`require_evidential`** — [ADR 0004](../adr/0004-grounds-are-weighed-by-evidential-depth.md).
  A support counts only if its citation chain reaches a stated fact, so a
  citation of an opinion stops being worth a citation of a measurement.

The motivation was the [live hidden-profile run](2026-09-01-live-hidden-profile.md),
whose second finding is that support is counted and grounds are not weighed:
scout's refutation sat in the transcript of every failed room at a citable
sequence and changed nothing, because killing a hypothesis meant one `!object`
per advocate and the room ran out of budget first.

## What the numbers say

5000 rooms, 5 agents, 4 options, `--noise 90`, seed 1. `hive+` is the tuned
policy; `hive+ref` is the same policy with `refutation_cap: Some(2)`; `hive+ev`
adds `require_evidential`. The control arms have refutation off, which also
stops the simulated members spending a turn on the move, so each arm differs
from the one above it in exactly one thing.

| arm | turns/ep | decided % | correct % |
| --- | --- | --- | --- |
| `ladder` | 1.00 | 100.0 | 57.6 |
| `vote` | 15.00 | 100.0 | 78.5 |
| `hive` | 6.16 | 89.7 | 73.3 |
| `hive+` | 6.75 | 99.4 | **82.1** |
| `hive+ref` | 8.99 | 88.6 | 75.0 |
| `hive+ev` | 10.29 | 60.8 | 55.9 |

Refutation costs seven points of accuracy and drops the arm below the
matched-budget vote. Evidential grounding costs twenty-six, and fails to decide
two episodes in five. Neither is close.

Every table below this one is at 500 rooms, which is the harness default; the
ordering is the same at both sample sizes.

**The damage scales with how noisy each member's private read is**, which is the
diagnostic that explains it:

| eval noise | `hive+` | `hive+ref` | `hive+ev` |
| --- | --- | --- | --- |
| ±30 | 100.0 | 100.0 | 100.0 |
| ±60 | 95.6 | 94.6 | 82.2 |
| ±90 | 78.8 | 72.8 | 53.8 |
| ±120 | 68.4 | 53.2 | 37.4 |

At ±30 every member can already see the answer, no refutation fires against the
truth, and the arms are identical. As noise rises the gap opens monotonically.

It is not an artifact of the majority quorum either. At the deadlock-prone
threshold of two, where `hive` and `hive+` coincide at 71.0%, `hive+ref` scores
65.0% and `hive+ev` 56.8%.

Finally, the grid search. `--sweep` now sweeps `refutation_cap` and
`require_evidential` alongside the five existing dimensions — 864 policies over
the same 500 rooms, 432,000 episodes. **Every policy in the top twelve has
refutation off and evidential grounding off.** The best policy overall scores
79.0% at 6.87 turns per episode, against 75.8% for the vote.

## Why it loses, as far as this can tell

The termination counts say where the turns went. Over 500 rooms, `hive+`
converges 495 times and exhausts 5; `hive+ref` converges 441 and exhausts 59;
`hive+ev` converges 298 and exhausts 202. Turns per episode go 6.84 → 8.95 →
10.28. Both mechanisms are narrowing rules, and a narrowing rule spends
budget.

But the noise table says the cost is not only budget. The reading with the most
support is this: **a refutation is global where an objection is local.** An
`!object` removes one advocate from one topic; a `!refute` caps the topic for
the whole room. A member firing one on a noisy private read therefore removes an
option for everybody, and at ±90 the noise is larger than the 60-point gap that
separates the true option from a decoy. The mechanism is neutral — it caps
whatever the refuter believes is wrong — and the more often that belief is
wrong, the more it costs. This is the fourth finding of the live run
("cross-inhibition fires, and it fires against the truth") made quantitative,
and amplified by the larger blast radius.

`require_evidential` fails differently and more simply: it starves the room. A
support whose chain does not reach an `!evidence` counts for nothing, so a room
that has not deposited facts cannot carry anything at all, and 202 of 500
episodes run out of budget.

## What this benchmark does not test

The case the mechanism was built for is not in it, and that has to be said
plainly rather than used as a defence.

The simulated task gives every member a noisy estimate of **every** option.
There is no decoy that accumulates support no individual's private read
contradicts, and no fact held by one member that overturns it — which is
precisely what a hidden profile is, and precisely the structure of the
`checkout-503` scenario where the failure was observed. On a task where each
member can already evaluate every option, evidence adds nothing that support
does not already carry, so a mechanism whose whole content is "weigh evidence
against support" has nothing to win and a real cost to pay.

So the honest statement is: **on the task this harness measures, both mechanisms
lose, decisively and reproducibly.** Whether they win on a hidden profile is
untested. Testing it needs a simulated hidden-profile arm — private facts rather
than private scores — which the harness does not have, and which is a larger
piece of work than either mechanism was.

## What was done about it

Per the spec's acceptance criterion:

- **Both knobs are off in `QuorumPolicy::DEFAULT`.** `refutation_cap` is
  `Option<u32>` and defaults to `None`, so the type says "off" rather than
  encoding it as an unreachable number. A default is not the place to carry a
  mechanism its own harness says costs accuracy.
- **The live protocol prompt does not teach `!refute`.** The criterion says a
  mechanism that does not beat the arm without it does not go on to the live
  prompt, and it did not.
- **The code stays.** It is specified, recorded in two ADRs, and covered. A
  host that has a hidden profile can switch it on, and the next person to ask
  this question starts from a mechanism and a number rather than from an
  argument.
- **A refutation is still recorded when the cap is `None`.**
  `TopicStanding::refuted_by` is populated either way; only the capping is
  opt-in. Turning the effect off should not erase from the standing that the
  room disagreed.

## Open items

1. A simulated hidden-profile arm: members holding disjoint private *facts*
   rather than noisy private *scores*, with a decoy the shared brief plants.
   Until that exists, the case for refutation is untested rather than refuted.
2. Whether a refutation should require the refuter to have deposited the
   evidence it cites. It does not today, and that would bound the blast radius
   the noise table measures — a member could then only cap a topic with a fact
   it put on the floor itself. This is the same shape as the grounded-objection
   rule, and it is cheap.
3. Whether `require_evidential` should widen rather than narrow: count an
   evidentially grounded support *double* instead of counting a socially
   grounded one as zero. Weighting cannot starve a room, and starvation is
   where 202 of its 500 episodes went.

## Reproducing

```sh
cargo run --release -p tinyhivemind-hive --example bench -- --episodes 5000
cargo run --release -p tinyhivemind-hive --example bench -- --noise 60
cargo run --release -p tinyhivemind-hive --example bench -- --quorum 2
cargo run --release -p tinyhivemind-hive --example bench -- --sweep
```

Everything is seeded; the same `--seed` gives the same rooms, the same private
evaluations, and the same transcripts.
