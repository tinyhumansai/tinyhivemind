# 2. Hive episodes are sequential, and visibility is the fan-out knob

- **Status:** Accepted
- **Date:** 2026-08-31

## Context

`crates/tinyteams-hive` adds group deliberation: a bounded episode in which
several agents on one desk propose, support, object, and reach a decision. A
"hive mind" is normally built as fan-out — publish a task, wake N agents, gather
their replies — and the charter forbids exactly that: *one message, one turn*,
`@everyone` is a list, not a broadcast.

That rule was written as a safety constraint. The literature says it is also the
better design, which is what makes this decision recordable rather than a
compromise:

- Sparse communication topologies match or beat fully connected ones in
  multi-agent debate at substantially lower cost, and over-dense connectivity is
  actively harmful (Li et al., Findings of EMNLP 2024). Demers et al. (PODC '87)
  give the same result formally for gossip.
- Conformity in LLM groups rises with interaction time and with a peer's
  apparent capability (Choi et al., Findings of ACL 2025; Weng et al., ICLR
  2025). Past a point more discussion increases *correlated* error, so
  convergence is a warning signal rather than a success signal.
- Parallel subagents win on genuinely decomposable tasks at roughly fifteen
  times the token cost, and are worse on tightly coupled work (Anthropic, 2025).
- Forty-one percent of observed multi-agent failures are specification and
  design faults rather than model faults — step repetition at 15.7%, unawareness
  of termination conditions at 12.4% (Cemri et al., MAST, arXiv:2503.13657).

Surveying implementations, every system that scales past a handful of agents
re-invents a bound on fan-out after the fact. MetaGPT is the clearest case:
`Environment.publish_message` defaults to `MESSAGE_ROUTE_TO_ALL` and wakes every
matching role concurrently, and the only thing preventing chaos is that the
shipped roles happen to have disjoint watch sets.

The one thing fan-out genuinely buys is independence — uncontaminated positions,
Surowiecki's condition that a shared transcript destroys, because the third agent
reads the first two before it answers.

## Decision

A hive episode is a bounded sequence of single turns. `HiveStep::Speak` carries
exactly one `HiveTurn`, the same way `MentionDispatchDecision::One` already
carries exactly one request, so the charter invariant is a type invariant.

Independence is bought as **visibility**, not as concurrency. A turn carries a
`Visibility`, and `project_for` filters what that turn's participant sees. The
blind round — round one formed without sight of peers, then revealed — restores
the independence condition for the price of a projection flag.

Consequently `tinyteams-hive` is a pure crate. It defines no port and no trait
for a host to implement: an episode is `step(state, transcript, …) -> HiveStep`,
and the host does its waiting through the `SessionLog`, `Selector`, and
`MentionTurnQueue` ports it already implements for P4 through P7. It is added to
`pure_crates` in `.github/scripts/assert-pure.sh`.

## Consequences

- The hive layer adds no host obligation beyond P4–P7. A host that has adopted
  those ports can run an episode without implementing anything new.
- Every hive decision is testable without a fixture, an executor, or a mock,
  because every one of them is a fold over arguments the caller already holds.
- Wall-clock latency of an episode is the sum of its turns, not the maximum. A
  host that needs parallel speculative work must express it as separate episodes
  and reconcile them itself; this crate will not grow a cohort primitive.
- Because the crate is pure, all arithmetic is fixed-point integer. Every
  payload type in this workspace derives `Eq` and pins an exact wire form, and
  floating point would break both determinism and those derives.
- If a future host demonstrates a decomposable workload where sequential
  episodes are the bottleneck, reversing this needs a new ADR and a new type —
  not a quiet relaxation of `HiveStep::Speak`.
