# 5. A blind round may be concurrent

- **Status:** Proposed
- **Date:** 2026-09-01
- **Amends:** [ADR 0002](0002-hive-episodes-are-sequential.md)

## Context

[ADR 0002](0002-hive-episodes-are-sequential.md) makes one message, one turn a
*type* invariant: `HiveStep::Speak` carries exactly one `HiveTurn` and there is
no variant that carries two. Its closing consequence names the door this record
walks through:

> If a future host demonstrates a decomposable workload where sequential
> episodes are the bottleneck, reversing this needs a new ADR and a new type —
> not a quiet relaxation of `HiveStep::Speak`.

The argument in 0002 is about **contaminated** fan-out: publish a task, wake N
agents, and the third reads the first two before it answers, which destroys the
independence Condorcet needs and which the conformity results say gets worse
with interaction time. Every citation in that record — sparse topologies
beating dense ones, conformity rising with interaction, correlated error — is
about agents *seeing each other*.

The blind round is the one part of an episode where, by construction, they do
not. `project_for` under `Visibility::Blind` hides every `SessionAuthor::Agent`
message above the watermark that is not the turn-holder's own. `visibility()`
keeps the round blind until every member has authored a live turn, and then
flips to `Full` permanently. So the blind round is exactly "each of the N
members speaks once, none seeing any other" — and the library spends N
sequential model calls producing it.

The benchmark says the round is worth having: blind opening 99.4% decided and
82.1% correct, against 100.0% and 58.0% at full visibility, a 24-point gap. It
is the single largest effect in the whole harness. It is also, at five members,
five turns of wall-clock and five turns off a budget of fifteen before the
argument starts.

## The claim, stated precisely

Let `T₁ … T_N` be the blind turns of a round, in the order `step` produced them.

**What is true.** The *input* to each blind turn is independent of every other
blind turn. `project_for` filters out peers' live agent messages, so the view
handed to `T_i` is identical under any permutation of the others: the operator's
task, that member's own prior work, and the conversation at or below the
watermark. Whatever `T_i` deposits, it would have deposited in any order.

**What is also true.** `standings` is permutation-invariant over the round,
*provided the whole round fits inside `QuorumPolicy::window`*. `TopicStanding`
accumulates `importance(kind)` per `(topic, agent)` pair and counts distinct
supporter ids; neither reads a sequence except to test window membership.
Cross-inhibition is applied after all support and is likewise keyed on author,
not order. So if `N ≤ window` — and the default window is 30 against desks of
three to eight — the room's standing after a blind round is the same whichever
order the round ran in.

**What is not true.** The *transcript* is a permutation, not a fixed value, and
two things read sequence rank rather than window membership. `salience` decays
by distance from `at`, and `bids` sums decayed standings, so the member who wins
the floor on the **first turn after** the round can differ with the order the
blind round was written in. The episode is therefore reproducible only up to the
order in which concurrent blind turns are committed — which is the host's to
fix, and which a host that commits them in roster order fixes for free.

## Decision

Add `HiveStep::SpeakBlind { turns: Vec<HiveTurn> }`, permitted **only** while
`visibility()` returns `Blind` and only under a new
`EpisodePolicy::concurrent_blind_round: bool` defaulting to `false`.

Every turn in the vector carries `Visibility::Blind`, and `step` returns the
variant only when the vector is the *complete* set of not-yet-heard members —
never a partial cohort, because a partial cohort is a cohort primitive and 0002
refuses one. Where the deliberating phase is concerned nothing changes:
`HiveStep::Speak` remains the only way to authorize a turn under
`Visibility::Full`, and there is no path from a `Full` step to more than one
speaker.

The host commits the resulting messages in roster order regardless of the order
its runtime completed them, which restores exact reproducibility and is stated
as a port-level obligation in the same place `SessionLog::read_before`'s cost
contract is stated.

## Proof obligation

This record is **Proposed**, not Accepted, and it should not be accepted before
the following are discharged as tests rather than as prose:

1. A property test: for any transcript and any permutation of a blind round's
   sequence assignment, `standings(read(transcript), at, policy)` is equal,
   given `N ≤ policy.quorum.window`. `TopicStanding` derives `Eq`, so this is a
   literal assertion.
2. A test that `step` refuses `SpeakBlind` for any strict subset of the
   unheard members, and refuses it entirely once `visibility()` is `Full`.
3. A benchmark arm at `concurrent_blind_round: true` showing decided% and
   correct% within noise of the sequential arm on the same seeds. If the
   concurrent round changes the numbers, the equivalence claim is wrong and this
   record should be rejected rather than patched.
4. A statement of what happens when `N > window`, which is a real
   configuration: either reject the policy combination as an error, or document
   that the equivalence does not hold there.

## Consequences

- **The charter's sentence survives intact where it earns its keep.** "One
  message, one turn" was written against fan-out that lets N agents contaminate
  each other. A blind round contaminates nobody; that is its definition and it
  is enforced by `project_for` rather than by convention. What this changes is
  not whether independence is preserved but whether the library spends N
  sequential model calls preserving something it has already proved
  order-independent.
- **`@everyone` is still a list.** Nothing here touches mentions, dispatch, or
  the responder ladder. `MentionDispatchDecision::One` is untouched, and no
  message triggers a turn it did not before — the blind round's membership was
  already determined by desk membership, not by a mention.
- **The crate stays pure and adds no port.** `SpeakBlind` is a return value. The
  host decides whether to run its turns concurrently, and a host that runs them
  serially gets the current behaviour with one extra allocation.
- **Wall-clock, not tokens.** The round costs the same N model calls either way.
  This buys latency and nothing else, and the benchmark's `ns/step` table will
  not move. A record that claimed a quality improvement here would be the exact
  confound [ADR 0002](0002-hive-episodes-are-sequential.md) and the
  [benchmark](../../wiki/Benchmarks.md) both warn about.
- **It widens a type that was deliberately narrow.** The cost is that
  "`HiveStep::Speak` carries one turn" stops being sufficient to read the
  invariant off the type, and becomes "`Speak` carries one turn, and the only
  other speaking variant is blind by construction". That is a real loss of
  legibility, and it is the strongest argument for rejecting this record.

## Related

- [ADR 0002](0002-hive-episodes-are-sequential.md), which this amends and does
  not supersede.
- [`../research/biology.md`](../research/biology.md) — the honeybee scouts this
  crate models do not take turns, and the Pais et al. model is a population
  dynamic in continuous time. Serialization is this library's addition, not the
  biology's.
