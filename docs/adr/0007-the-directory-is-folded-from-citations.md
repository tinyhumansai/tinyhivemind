# 7. The directory is folded from citations, and the host's affinity is a prior rather than an authority

- **Status:** Accepted
- **Date:** 2026-09-05

## Context

`AgentThreshold.affinity` has been in the crate since P8 and nothing has ever
written it. It is a `Vec<(TopicId, u8)>` a host supplies, read by
`AgentThreshold::relevance` and folded into the salience term as a per-member
multiplier. Its doc comment claimed the pair of scalars bought "emergent
specialisation"; that claim was false, and this change corrects it as well as
adding the mechanism that would make it true.

So the library had two ways to say who should speak — the salience field, and a
static configuration — and no way to derive who *knows*. In a hidden profile
that is the difference between a decision and a decoy: the member holding the
unique fact deposits it once, nobody cites it, and the salience field never
points at that member again.

Wegner's transactive memory says the missing structure is a **directory**, and
Lewis gives two of its factors in a form an attributed transcript can estimate:
specialisation and credibility. Hollingshead separates the diffuse cue (a role
label; a configured affinity) from the specific one (observed experience; a
folded record of grounded deposits), and finds the diffuse cue matters less as
shared history accumulates.

Three questions had to be answered.

**Where does the directory live?** The tempting answer is `EpisodeState`: carry
it, update it per turn, persist it across episodes. That is a second journal in
miniature, it has to be invalidated when the transcript is re-paged or a
message is superseded, and an iterated update is not commutative — two hosts
that folded the same transcript in different orders would disagree.

**What counts as knowing?** The cheap estimator is speech. It is also the
wrong one, and it is the estimator DyLAN, `description`-matching subagent
routers, and every "ask the model who should answer" baseline effectively use.
Counting speech makes who spoke into who is thought to know.

**Does the host's affinity still bind?** If a configured affinity outranks the
fold, the mechanism is decorative; if the fold ignores it, a host that genuinely
knows its roster loses the only channel it had.

## Decision

**The directory is a fold over the transcript, not stored state.**
`directory(traces, at, policy, priors)` is a pure function returning an owned
`Directory`, computed fresh on every `step`. It is order-independent and
idempotent on `(sequence, offset)`, exactly as `standings` is. Nothing persists
between episodes, so a wrong estimate cannot follow a member forward.

**Knowing is estimated from grounded deposits and the citations they drew, not
from speech.** Specialisation counts a member's own `Evidence` (full weight)
and grounded `Propose`/`Support`/`Refute` (partial), decayed by sequence
distance. Credibility counts *other* members' citations of those deposits, on
the topic the citer named, and is debited by other members' objections. A
member's own citations of itself earn nothing. Ungrounded assertion earns
nothing at all, so the cheapest way to inflate an estimate is closed.

**The host's affinity is a prior.** `AgentThreshold::declared_relevance`
returns `Option<u8>` — the honest shape, where the existing `relevance` returns
a neutral 50 for a topic nobody declared. An undeclared topic contributes no
prior rather than half of one. A declared one enters the weight through
`policy.prior`, a third of the credibility weight, alongside the two folded
terms rather than above them.

**`Knows` sits between `Dissent` and `Quiet`.** Being addressed is a fact about
this message and outranks any estimate. Dissent outranks it because an
unbroken deadlock terminates the episode: a routing preference must not be able
to suppress the one member who could break it. Below that, an uncited fact
holder outranks the equality guard, because "somebody has not spoken" is a
weaker reason than "this member knows and has not said so".

The bonus stops the moment its holder deposits a trace on the topic. It buys a
fact its first hearing and nothing after.

**It ships off by default.** `EpisodePolicy::DEFAULT` carries
`directory: None` and `defer_cap: None` until the benchmark scores the arm.
This is the same discipline `refutation_cap` and `require_evidential` are
under, and for the same reason: two mechanisms in this crate have already been
measured, lost, and been left opt-in rather than quietly defaulted on.

**An old policy payload is rejected loudly.** Both new fields are
required-but-nullable through a `deserialize_required_*` shim, so an
`EpisodePolicy` serialized before this change fails to decode rather than
silently meaning "off". A host that wants the mechanism off writes
`"directory": null`.

**`BidContext` takes the quorum policy, not a window.** The field was
`window: u32`; it is now `quorum: &QuorumPolicy`, and the window is read from
it. The *value* is unchanged — an episode has always passed its own
`policy.quorum.window` — but the change is source-breaking for a host that
constructs a `BidContext` by hand rather than going through `step`. It is worth
the break because `bids` also has to ask whether a standing has `carried`, and
that is a question about the whole policy; passing the window alone meant
carrying a second, partial view of a policy the caller already had, which is
exactly how the two could have drifted apart.

That leaves two windows in the mechanism, and they answer different questions.
`QuorumPolicy.window` decides which deferrals are live enough to count against
`defer_cap` and to promote a contested topic; `DirectoryPolicy.window` decides
which are live enough to zero their author's weight. Both default to `30`, so
they coincide out of the box, and a host that widens one without the other gets
deferrals that do half their job.

## Consequences

**Positive.** The estimator's inputs are grounds and other members' judgements,
both of which cost something to produce. There is no new port, no new stored
state, and no new invalidation path. `Directory::entries()` and
`Directory::lines()` are public, so a benchmark can audit the estimate and a
host can render it into a prompt. Correcting `affinity` from "emergent
specialisation" to "a host-supplied prior" also fixes a false claim in the
crate docs, the P8 spec, and the wiki.

**Negative, and unresolved.** The circularity is bounded, not removed: a member
who speaks more has more chances to deposit and be cited. Mutual citation
between two members raises both for free and nothing detects a ring. A six- or
seven-turn episode is a short window for any estimator, and on a uniform-
expertise room the expected gain is exactly zero. The mechanism therefore
carries a published obligation — the rank correlation between directory weight
and speech share, reported next to accuracy — and it is allowed to lose.

One concrete leak is already known and is recorded as an open question in the
spec: the credibility term credits every live trace that cites another
member's deposit, and does not exclude an `Object`. An `!object #t >N ^N` —
objecting to a sequence while citing it — therefore credits the member it
objects to, and under the default weights that credit very nearly cancels the
discredit. It is defensible (the objection did engage with the fact) and it is
also a cheap way for two members to launder credibility past the debit, and it
has not been measured.

**On `!defer`.** It is the crate's eighth verb, and every added verb here has
lost: `!refute` cost 7 points, `require_evidential` cost 26. It is included
because it is the only move that turns a turn spent on nothing into a turn that
deposits directory evidence — the tremble dance rather than the waggle dance —
and it is bounded by `defer_cap` because an unbounded deferral chain is step
repetition, the single most common observed multi-agent failure.

## Alternatives considered

- **Threshold reinforcement.** Lower `θ` per topic on the topics a member acts
  on, clamped. It is the literature's own answer and it needs cross-episode
  persistence this crate deliberately does not own; at 6.75 turns per episode
  there is nothing for it to learn from.
- **A `Directory` port.** A host-supplied lookup, so an application could bring
  its own expertise model. It would be a callback into the host, which is the
  layering violation this workspace exists to fix.
- **Expertise-directed referral.** Route across desks by directory weight. The
  directory is folded from *this* conversation, so members of another desk weigh
  zero, and a returned answer with no rebuttal is a cascade seed. Deferred.
- **Storing the directory in `EpisodeState`.** Rejected above: not commutative,
  needs invalidation, and is a second journal in miniature.
