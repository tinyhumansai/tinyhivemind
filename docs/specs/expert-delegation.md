# Expert delegation: a folded directory, `BidReason::Knows`, and `!defer`

- **Status:** Accepted
- **Owner:** `crates/tinyhivemind-hive`
- **Reading:** [`../research/delegation.md`](../research/delegation.md)
- **Decision:** [ADR 0007](../adr/0007-the-directory-is-folded-from-citations.md)

## Problem

The library can express *who is here* and *who spoke*. It cannot express *who
knows*. `RosterMember` is `{id, name}`. The one expertise-shaped field,
`AgentThreshold.affinity`, is host-supplied and never written by anything in
the workspace, so a room that nobody configured routes on recency and trace
importance alone. The attention market therefore gives the floor to whoever the
salience field happens to point at, which in a hidden profile is reliably the
member restating what everyone already knows.

That is the second finding of
[the live hidden-profile run](../experiments/2026-09-01-live-hidden-profile.md):
the member holding the fact that overturns the decoy is in the room, has
already deposited the fact, and never wins another turn to press it.

Transactive-memory theory names the missing structure exactly. Wegner (1986)
locates a group's memory in the **directory** — who knows what — rather than in
the contents. Lewis (2003) validates it as three factors, of which two can be
estimated from an attributed transcript: **specialisation** (this member's
deposits cluster on this topic) and **credibility** (other members build on
this member's deposits). Hollingshead (2000) separates a *diffuse* cue such as
a role label from a *specific* cue such as observed experience, and finds the
diffuse cue's influence falls as a team accumulates shared history. A
host-supplied `affinity` is a diffuse cue. A folded record of grounded deposits
is a specific one.

## Goals and non-goals

**Goals.** Derive a per-`(agent, topic)` weight from the transcript as a pure,
order-independent, fixed-point fold. Feed it into the attention market as one
new bid reason. Give a member a way to say "not mine" that costs one turn and
returns one bit of directory evidence. Keep everything off unless a host asks
for it.

**Non-goals.** No new port. No stored directory: it is folded fresh on every
step from traces the caller already holds, so there is nothing to invalidate
and no second journal. No cross-desk directory — a member of another desk has
deposited nothing in this transcript and weighs zero. No fan-out: `Knows` moves
*which* single member takes the floor and never how many do. No learned
weights; every constant here is ordinal and stated.

## Proposed behavior

### The two estimators

`directory(traces, at, policy, priors) -> Result<Directory>` folds one entry
per `(agent, topic)` pair that any term reaches.

All arithmetic is fixed-point thousandths in saturating `i64`.

Let `d(s) = decay(at.0 - s.0, policy.half_life)`, the same integer halving the
salience field uses, in `0..=1000`.

The **live set** is every trace with `at - policy.window <= sequence <= at`,
sorted and deduplicated by `(sequence, offset)` — exactly as `standings` does,
which is what makes the fold commutative and idempotent.

A trace's **deposit** is what it put on the floor that another member could
build on:

| trace | deposit |
| --- | --- |
| `Evidence` | `1000` |
| `Propose`, `Support`, `Refute`, and grounded | `600` |
| anything else, including any ungrounded position | `0` |

`Question`, `Commit`, `Object` and `Defer` deposit nothing. A conclusion
offered without grounds deposits nothing either: it is the same second-class
treatment `require_grounded` gives support, for the same reason.

**Specialisation** is the decayed weight of a member's own topiced deposits:

```text
specialisation(a, t) = Σ  deposit(x) · d(x.sequence) / 1000
                     x ∈ live, author(x) = a, topic(x) = Some(t)
```

**Credibility** is what the *room* did with them. For every live trace `c`
authored by some `b ≠ a` with `topic(c) = Some(t)`, where `c` cites a sequence
at which `a` deposited anything at all (any topic, or none):

```text
credibility(a, t) += 1000 · d(c.sequence) / 1000
```

Self-citation earns nothing, which is the whole point: credibility is a
judgement other members made. A `Refute` counts as a citer — a refutation that
builds on `a`'s fact to kill a topic is the room *using* `a`'s fact, and the
topic it names is the topic `a` is credible on.

An objection debits it. For every live `Object` authored by `b ≠ a` whose
`target` is a sequence at which `a` deposited something attributed to `t`
(directly by its own topic, or through a citer that named `t`):

```text
credibility(a, t) -= policy.discredit · 1000 · d(o.sequence) / 10_000
```

Credibility is clamped at zero **once**, after every credit and every debit in
the live set has been folded — not after each term. Clamping per term would
make the result depend on the order the traces happened to be folded in: a
debit that landed before the citation it cancels would be truncated at zero and
the citation would then start again from there, where the same two traces in
the other order net out. The fold is defined over a sorted, deduplicated live
set precisely so no such ordering exists, and one clamp at the end is what
keeps that true. What survives is the intended guarantee — no amount of
objecting drives a member's credibility negative — without buying it at the
cost of commutativity.

**Prior.** `prior(a, t) = declared_relevance(t).map(|r| min(r, 100) · 10)`,
`0` when the member declared nothing for that topic. Note that this is *not*
`AgentThreshold::relevance`, which returns a neutral 50 for an undeclared
topic: a member who was never configured contributes no prior rather than half
of one, because "unknown" and "moderately expert" are different claims.

**Weight.**

```text
weight = clamp((specialisation · policy.specialisation
              + credibility    · policy.credibility
              + prior          · policy.prior) / 10, 0, WEIGHT_CEILING)
```

`WEIGHT_CEILING` is `1_000_000`, so a long transcript saturates rather than
growing without bound — the same clamp MAX–MIN Ant System uses against
premature convergence.

A live `Defer` by `a` naming `t` sets `weight(a, t) = 0` outright. It is a
statement about the member's own competence and it outranks the estimate.

An entry whose specialisation, credibility and prior are all zero is dropped,
so `Directory::entries()` names only holders.

### `DirectoryPolicy`

```rust
pub struct DirectoryPolicy {
    pub half_life: u32,      // sequence distance at which a deposit halves
    pub specialisation: u16, // weight on own deposits, in tenths
    pub credibility: u16,    // weight on citations by others, in tenths
    pub prior: u16,          // weight on host-declared affinity, in tenths
    pub discredit: u16,      // objection debit, in tenths of a citation
    pub window: u32,         // how many sequences back a deposit counts
    pub floor: i64,          // weight at which `knows` becomes true
}
```

`DEFAULT` is `half_life: 20, specialisation: 30, credibility: 20, prior: 10,
discredit: 20, window: 30, floor: 1_000`. Specialisation outweighs credibility
because a deposit is a fact the member actually stated and a citation is a
second member's opinion about it; the prior is a third of credibility because
it is the diffuse cue.

`half_life` or `window` of zero is [`Error::ZeroDirectoryHalfLife`] or
[`Error::ZeroDirectoryWindow`]; two priors naming one agent is
`Error::DuplicateAgentThreshold`, the same failure `bids` reports.

### Firing rule

`BidReason::Knows` sits between `Dissent` and `Quiet`, so the precedence chain
is:

```text
Addressed > Dissent > Knows > Quiet > Salience
```

Being addressed is a fact about *this* message and outranks an estimate.
Dissent outranks it because a deadlock nobody breaks terminates the episode,
and a directory that could suppress the one member able to break it would trade
a decision for a routing preference.

`Knows` adds `KNOWS_BONUS = 1_250` — above `QUIET_BONUS` (1000) and below
`DISSENT_BONUS` (1500), so the ordering of the bonuses matches the ordering of
the reasons.

It fires for a member when all three hold, on the **contested topic**:

1. the member is `directory.top_among(topic, members)` — the highest-weighted
   holder *on this desk*, ties broken by desk order;
2. `directory.knows(member, topic, policy)`, that is `weight >= policy.floor`;
3. the member has taken no **position** on that topic in the episode — no
   `Propose`, `Support`, `Object`, `Refute`, `Commit` or `Defer` naming it.
   `Evidence` and `Question` do not count.

Condition 3 is what makes this delegation rather than amplification: the bonus
buys an unheard fact its hearing, and stops paying the moment the holder argues
the topic.

*Position* rather than *any trace* is deliberate, and it is the whole case the
mechanism exists for. In a hidden profile the holder has already deposited the
fact and nobody cited it; treating that deposit as having spoken would make
`Knows` unreachable in exactly the situation it was built for. A member that
keeps depositing grounds without ever taking a position can keep drawing the
bonus, which the dominance guard and the rising speak cost damp rather than
forbid.

The **contested topic** is, in order:

1. the live `Defer` with the highest `(sequence, offset)` that names a topic,
   provided the live defer count is below `defer_cap`;
2. otherwise the standing with the greatest `support` that has not `carried`,
   ties broken by first-advocated order;
3. otherwise `None`, and `Knows` cannot fire.

*Highest address*, not *last delivered*: `bids` sorts and deduplicates its
traces by `(sequence, offset)` before reading any of them, exactly as
`standings` and `directory` do, so a medium that reorders or redelivers a
message cannot move the contested topic or spend a member's `defer_cap` twice
on one deferral. The cap counts distinct deferrals, after that deduplication.

Without a directory in the `BidContext` there is no contested topic and the
reason is unreachable.

#### Two windows decide what a *live* deferral is

They are not the same window, and it is worth being explicit about which does
what:

- `BidContext.quorum.window` bounds the deferrals `bids` counts against
  `defer_cap` and promotes to the contested topic, and the grounded share the
  dominance guard measures.
- `DirectoryPolicy.window` bounds the live set the directory folds, and so
  which `Defer` zeroes its author's weight on a topic.

A deferral inside one window and outside the other therefore does half the job:
outside the quorum window it no longer promotes its topic but still zeroes its
author's weight; outside the directory window the reverse. `QuorumPolicy`'s and
`DirectoryPolicy`'s defaults are both `30`, so the two coincide unless a host
moves one, and a host that moves one should move both or know why not.

#### `BidContext` carries the whole `QuorumPolicy`

`BidContext` used to carry a bare `window: u32`. It now carries
`quorum: &QuorumPolicy`, from which the window is read. The value passed is the
same one — an episode has always handed `bids` its own `policy.quorum.window` —
but a host that constructs a `BidContext` by hand has to pass the policy
instead of the number. This is source-breaking and deliberate: `bids` also has
to ask whether a standing has `carried`, which is a question about the whole
policy rather than about its window, and threading two views of one policy was
how the window and the carry rule could have drifted apart.

### An uncited fact must carry `#topic`

Specialisation reads `topic(x) = Some(t)`. `!evidence` with no `#topic` earns
its author credibility only through whatever *other* member later cites it —
which in a hidden profile is precisely the member who never appears. A fact
nobody has cited therefore has to name its own topic to be routable at all.

This is a constraint on the prompt, not only on the fold: a host rendering the
grammar into a live room must say that `!evidence` takes a `#topic`, and the
`examples/bench` agent prompt does. It is stated here because a mechanism whose
input never appears is indistinguishable from a mechanism that does not work.

### `!defer`

```text
!defer #topic [^N] free text
```

The topic is required and the marker fails closed without one, exactly as
`!refute` does — a deferral that names nothing is not a deferral.

Wire spelling `"defer"`. `importance(Defer) = 200`, below `Question`'s 300: it
is the least floor-moving thing a member can say. It moves no support and
touches no standing. It does two things: it zeroes its author's directory
weight on that topic, and it promotes that topic to the contested topic so the
next bid routes to whoever *does* hold it.

`defer_cap` bounds the chain. With a cap, once that many live defers exist the
deferred topic stops being promoted and deliberation resumes on the standings;
without one, the chain is bounded only by `turn_budget`. `Some(0)` is
[`Error::ZeroDeferCap`] — it would cap the mechanism before anyone could use
it, which is a configuration error rather than a way to switch it off. `None`
is how a host switches it off.

### Episode policy

`EpisodePolicy` gains two required-but-nullable fields:

```rust
pub directory: Option<DirectoryPolicy>,
pub defer_cap: Option<u32>,
```

Both are `None` in `DEFAULT`. Both use the `deserialize_required_*` shim
`refutation_cap` established: an absent key is a decode error rather than a
silent "off", so a host that means to leave the mechanism off has to say so.

`step` folds the directory at the same `at` as standings when
`policy.directory` is `Some`, and hands it to the attention market. The
signature is unchanged and no port is added.

## Invariants and constraints

- The fold is pure, order-independent and idempotent on `(sequence, offset)`,
  and fixed-point throughout.
- A member's own citations never raise their own weight.
- Credibility never goes below zero, and weight never exceeds
  `WEIGHT_CEILING`.
- `Knows` never fires for a member that has already spoken on the topic.
- With `directory: None` and `defer_cap: None`, `step` is bit-identical to the
  P9 behaviour. This is a regression test, not a claim.
- Nothing about the directory is stored. It is folded from the transcript on
  every step.

## The circularity hazard

Who spoke becomes who is thought to know. Every transcript-folded expertise
estimate shares this defect with DyLAN's importance score and with matching a
task against a subagent description: the estimator's input is the output of the
policy it feeds. Left alone it is an information cascade with a routing table
attached.

Three things bound it here, and none of them removes it:

1. **Speech is not the estimator.** A deposit only counts if it carries grounds
   or is a stated fact, and credibility only accrues from *other* members'
   citations. Talking more, ungrounded, earns nothing.
2. **The bonus stops.** `Knows` pays only until the holder has spoken on the
   topic, so it cannot compound within an episode.
3. **Nothing persists.** The directory is refolded per step and dies with the
   episode, so a wrong estimate cannot follow a member into the next one.

**The rank-correlation obligation.** The benchmark must report the rank
correlation between a member's final directory weight and its share of the
episode's turns, alongside accuracy. `Directory::entries()` is public for
exactly this reason. A directory that correlates near-perfectly with speech
share has learned nothing except who talked, and the mechanism should be
reported as having failed even if accuracy rose.

## Acceptance criteria

Written before the numbers, per
[`../research/delegation.md`](../research/delegation.md):

1. `BidReason::Knows` gives the floor to the holder of an uncited relevant fact
   in a constructed hidden-profile transcript, and does not with
   `directory: None`. *(This is criterion 6 of
   [`shared-medium-schema.md`](shared-medium-schema.md).)*
2. The mechanism must be able to lose, and the loss must be published. The
   bench arm and scenario family are fixed before any tuning.
3. `vote` gets the same turn budget; results are reported at equal turns and as
   turns-to-decision at equal accuracy.
4. A mechanism that helps hidden profiles but costs more than two points on the
   uniform 5000-room bench ships off by default.
5. Directory circularity is reported as the rank correlation between directory
   weight and speech share.
6. The predicted result on the uniform bench is **zero**: with homogeneous
   expertise there is nothing to route on.

Until the benchmark scores it, `EpisodePolicy::DEFAULT` carries
`directory: None` and `defer_cap: None`.

## Open questions

- **Does `!defer` pay for itself?** A turn costs a full turn, where a cheap
  model call in a cost-aware cascade costs about 1% of an expensive one.
  Miscalibrated deferral is the dominant failure of escalation systems, and a
  defer chain is a step-repetition surface — the single most common observed
  multi-agent failure. `defer_cap` bounds it; whether the bound is enough is an
  empirical question.
- **Should the directory be rendered into the room?** Stasser's expert-role
  assignment raises unique-item sampling modestly by *announcing* who knows
  what. `Directory::lines()` exists so a host can paste it into a prompt, but
  nothing here does, and rendering also renders the host's prior as if it were
  earned.
- **Is mutual citation gameable?** Two members that cite each other raise each
  other's credibility for free. Nothing here detects a citation ring.
- **An objection that cites its own target pays its target.** The credibility
  fold credits *every* live trace that cites another member's deposit, and an
  `Object` is not excluded. So `!object #t >N ^N`, objecting to sequence `N`
  while citing it, credits the objected-to member on `#t` at the same time as
  it debits them. Under the default weights the credit very nearly cancels the
  debit: on a four-message probe the objected-to member ends on `925`
  thousandths of credibility where the same objection written bare, `!object #t
  >N`, leaves them on `0`. Whether that is a bug or the honest reading of "the
  objection engaged with the fact" is unsettled — an objection that quotes what
  it is objecting to *has* used the deposit — but it is at minimum a cheap way
  for two members to launder credibility past the discredit term, and it should
  be measured before the directory ships on by default.
