# The directory

Who knows what, folded from one transcript. Wegner's transactive memory: a
group's memory is the *directory* — the index of who holds what — rather than
the contents.

## Design

`directory(traces, at, policy, priors)` is a fold, in the same shape as
`quorum::standings`. It takes traces the caller already holds and returns an
owned `Directory`. It never appends, never waits, and stores nothing.

There is deliberately no stored directory and no per-turn update. An iterated
update is not commutative — two hosts folding the same transcript in different
orders would disagree — and stored state would have to be invalidated whenever
the transcript is re-paged, superseded, or digested. Refolding is cheap and
always agrees with the transcript.

```text
live set = traces in [at - window, at], sorted + deduped by (sequence, offset)
  ├─ specialisation(a, t)  own topiced deposits, decayed
  ├─ credibility(a, t)     other members' citations of a's deposits, on t,
  │                        less this policy's debit per objection, clamped ≥ 0
  └─ prior(a, t)           what the host declared, or nothing
     └─ weight = clamp((spec·wₛ + cred·w_c + prior·wₚ)/10, 0, WEIGHT_CEILING)
        └─ a live !defer by a on t sets weight(a, t) = 0
```

### What counts as a deposit

| trace | deposit |
| --- | --- |
| `Evidence` | 1000 |
| `Propose`, `Support`, `Refute`, grounded | 600 |
| everything else, and any ungrounded position | 0 |

Speech is not the estimator. An ungrounded assertion is the cheapest thing an
agent can emit, so it deposits nothing — the same second-class treatment
`require_grounded` gives support, for the same reason.

### Why credibility is a separate term

Specialisation is what a member claims about itself; credibility is what the
room did with it. Self-citation earns nothing, so a member cannot raise its own
credibility at all. A `Refute` citing a member's fact still credits that
member: killing a topic with somebody's fact is the room *using* the fact.

An objection debits, and the result is clamped at zero, so no volume of
objecting drives a member negative.

### The host's affinity is a prior

`AgentThreshold::affinity` is a **diffuse cue** in Hollingshead's sense — a
role label rather than observed experience — and its influence should fall as a
team accumulates shared history. It enters through `policy.prior`, a third of
the credibility weight, alongside the folded terms rather than above them.

The fold reads `AgentThreshold::declared_relevance`, which returns `Option<u8>`
and therefore distinguishes "undeclared" from "neutral". `relevance`, which the
salience multiplier uses, substitutes 50 for an undeclared topic; using that
here would make every unconfigured roster look moderately expert in everything.

## The circularity hazard

**Who spoke becomes who is thought to know.** DyLAN's importance score, a
subagent router matching a task against a `description`, and any
transcript-folded affinity share this defect: the estimator's input is the
output of the policy it feeds. Left alone it is an information cascade with a
routing table attached.

Three things bound it here, and none of them removes it:

1. **Speech is not the estimator.** Only grounds and stated facts deposit, and
   credibility accrues only from *other* members' citations.
2. **The bonus stops.** `BidReason::Knows` pays only until the holder has
   deposited on the topic, so it cannot compound within an episode.
3. **Nothing persists.** The fold dies with the episode, so a wrong estimate
   cannot follow a member into the next one.

Two things it does not bound: a citation ring between two members raises both
for free, and a member who wins more turns has more chances to deposit.

`entries()` is public **for this reason**. A harness must be able to
rank-correlate directory weight against speech share and report it next to
accuracy; a directory that correlates near-perfectly with who talked has
learned nothing, and should be reported as having failed even if accuracy rose.

## Public surface

| Item | Purpose |
| --- | --- |
| `directory` | The fold. Returns one owned `Directory`. |
| `Directory` | Entries in `(topic, agent_id)` order; only holders appear. |
| `DirectoryEntry` | `agent_id`, `topic`, `specialisation`, `credibility`, `weight`. |
| `DirectoryPolicy` | Half-life, the three term weights, the objection debit, window, floor. |
| `WEIGHT_CEILING` | The saturation bound, `1_000_000`. |
| `Directory::entries` | The whole slice, for auditing the estimate. |
| `Directory::topics` | Every held topic, in topic order. |
| `Directory::weight` | One pair's weight, or zero. |
| `Directory::knows` | Whether that weight reaches `policy.floor`. |
| `Directory::top` | The highest holder, ties by agent id. |
| `Directory::top_among` | The highest holder on a desk, ties by desk order. |
| `Directory::lines` | One rendered line per topic, for a prompt. |

## Operational constraints

- **Order-independent and idempotent** on `(sequence, offset)`, exactly as
  `standings` is. A redelivered trace folds to the same directory.
- **Fixed-point throughout.** Every quantity is thousandths in saturating
  `i64`, so `Eq` holds and the fold is reproducible.
- **Weight saturates at `WEIGHT_CEILING`.** A long transcript cannot make one
  member's lead unreachable — the clamp MAX–MIN Ant System uses against
  premature convergence.
- **An uncited fact must carry `#topic`.** Untopiced `!evidence` earns its
  author credibility only through a later citer, which in a hidden profile is
  exactly the member who never appears. A host rendering the grammar into a
  live room has to say so.
- **Off by default.** `EpisodePolicy::DEFAULT` carries `directory: None` until
  the benchmark scores the arm. See
  [`../../../../docs/adr/0007-the-directory-is-folded-from-citations.md`](../../../../docs/adr/0007-the-directory-is-folded-from-citations.md).
