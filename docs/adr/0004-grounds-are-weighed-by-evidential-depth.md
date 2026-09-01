# 4. Grounds are weighed by evidential depth, not counted

- **Status:** Accepted, shipped **off by default**
- **Date:** 2026-09-01

## Context

`Trace::grounded()` is `!cites.is_empty()`. Under `QuorumPolicy::require_grounded`
a `!support` with no citation contributes to neither `supporters` nor `support`,
and [`../specs/hive-mind.md`](../specs/hive-mind.md) gives the reason: "a
conclusion offered without grounds is what lets an information cascade form, so
it is second-class by construction."

That is the right diagnosis and an incomplete remedy. A citation is not evidence.
`!support #retries ^6` where message 6 is itself a `!support` is a citation of an
opinion, and it counts exactly as much as a citation of a measurement.

The cascade literature says why this matters, precisely. Bikhchandani,
Hirshleifer & Welch, *Journal of Political Economy* 100(5):992–1026 (1992): a
cascade forms when it is optimal to follow predecessors **regardless of your own
private signal**, and the root cause is that agents observe *actions, not the
evidence behind them*. Once the third agent follows the first two, her action
carries zero information and the public belief freezes — and no number of later
arrivals corrects it. Requiring a citation does not break that. It requires the
third agent to *point at* the first two, which she was doing anyway.

The [live run](../experiments/2026-09-01-live-hidden-profile.md) shows both
failure shapes. Finding 2: three grounded supporters carried the decoy while the
refutation sat uncited. Finding 3 is sharper — the auditor's concurrency
evidence was *compatible with both* the true cause and the decoy, and once the
auditor proposed `#retries` from it, "every later member cited that message as
grounds for the decoy". Pooled information does not help if it lands under the
wrong heading, and every citation in that chain was formally grounded.

The insect literature has the distinction as a mechanism rather than a caution.
In *Temnothorax* house-hunting (Pratt, *Behavioral Ecology* 16(2):488–496, 2005)
a pre-quorum **tandem run** recruits one ant who *independently evaluates the
site*; a post-quorum **social carry** moves a passive ant who evaluates nothing.
Both increase the count at the site. Only the first adds a conditionally
independent signal. Marshall et al., *eLife* 8:e40368 (2019), close the loop:
quorum pooling is optimal *for independent judgements*, and the correlated ones
have to be identified to be discounted.

## Decision

Classify each `Support` by the **root kind of its citation chain**, and let
policy require the evidential class.

Resolve a support's `cites` transitively through the traces in window: follow
each cited sequence to the trace at that sequence and recur, stopping at a trace
with no citations of its own, at a `TraceKind::Evidence`, or on revisiting a
sequence already seen. A support is:

- **evidentially grounded** if any chain reaches an `Evidence` trace;
- **socially grounded** otherwise — its chain bottoms out in `Support`,
  `Propose`, `Question`, or a citation outside the window.

`QuorumPolicy` gains `require_evidential: bool`. When set, a socially grounded
support contributes to neither `supporters` nor `support`, exactly as an
ungrounded one does under `require_grounded`. `require_evidential` implies
`require_grounded`; setting it without the other is accepted and behaves as if
both were set, because an uncited support has no chain to resolve.

Cycle handling is a visited set on sequences, so the fold terminates on a
transcript in which two supports cite each other. A chain that leaves the window
is *not* followed outside it: the window is what makes the fold local and
idempotent for a late-joining participant, and reaching past it would make a
member's standing depend on how far back it happened to have paged.

`require_evidential` defaults to **false**. It is a strictly narrowing rule, it
can starve a room that has not deposited evidence, and whether it pays is an
empirical question the benchmark answers rather than one this ADR settles.

## Consequences

- **The first turn of an argument becomes structurally different from the
  tenth.** With `require_evidential`, a chain of agreement built on no
  measurement carries nobody. That is the cascade condition, refused.
- **It cannot tell a *good* citation from a bad one.** The library resolves the
  chain's *kind*; it does not read the evidence and judge whether it supports the
  claim. Finding 3 — a fact attaching to the wrong hypothesis — is *narrowed* by
  this rule, not fixed: the auditor's evidence is real evidence, so a support
  citing it is evidentially grounded whichever topic it names. Fixing that would
  require reading the text, which a pure fold cannot do and which
  [ADR 0002](0002-hive-episodes-are-sequential.md)'s determinism argument says
  should not be smuggled in behind a model call.
- **`Evidence` becomes load-bearing, and can be gamed.** A member can make any
  support evidential by first depositing an `!evidence` line of no content. The
  library's answer is that this costs a turn out of a small budget, and that
  `dominance_cap` measures share in *grounded, supported* contributions
  specifically because raw count is a proxy an agent can inflate. It is a real
  limit and belongs in the benchmark's "what this does not show".
- **The window bound is now doing two jobs.** It bounds the count, and it bounds
  the chain resolution. A short window makes more supports read as socially
  grounded, because their chains fall off the edge. That interaction is worth a
  sweep dimension rather than a fixed constant.
- **Cost is bounded.** Resolution is over traces already folded and already
  sorted by `(sequence, offset)`; with a visited set the whole pass is linear in
  citations within the window.

## Related

- [ADR 0003](0003-refutation-links-evidence-to-a-topic.md), the other half.
- [`../specs/refutation-and-grounds.md`](../specs/refutation-and-grounds.md).
- [`../research/biology.md`](../research/biology.md), for the cascade condition
  and the tandem-run/social-carry distinction.
