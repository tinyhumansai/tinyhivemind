# What the scale run found, and what it cost to fix

**Date** 2026-09-10
**Status** Recorded
**Follows** [A thousand agents, and the one thing that breaks first](2026-09-10-hive-at-scale.md)
**Code** `hive-at-scale`, on 28 cores. Simulated participants throughout, so
every number reproduces from its seed.

The scale run answered its own question and left six others behind it. Five are
fixed in the library and one in the harness; this is what each was, what the fix
was, and what it measures at.

Two of the six are *negative* results and are reported as such. Nothing here
changes a recorded number: `ladder 57.6 · vote 78.5 · hive 73.3 · hive+ 82.1`
and `swarm 78.0 · swarm° 79.0 · pooled 75.5` all reproduce exactly, which is the
regression test for the whole change.

---

## 1. The defaults are wrong above a dozen members, silently

`EpisodePolicy::DEFAULT` states three numbers absolutely where the quantity they
bound scales with the room. Each fails without saying so.

- `turn_budget: 12` is fewer turns than a room of thirteen has members. Under
  `blind_round`, visibility lifts only once *every* member has authored a live
  row — so such a room stays `Visibility::Blind` for its entire episode. It
  takes turns and never deliberates. The mechanism the crate exists for does not
  engage, and the return value says nothing about it.
- `quorum.threshold: 2` is two supporters whether the desk holds five members or
  a thousand.
- `weights.half_life: 20` is twenty rows against an opening round that is
  `members` rows long. At 256 members a trace from the opening round is twelve
  half-lives old before the round has finished — worth about one seven-thousandth
  of a fresh one. The opening round is the one carrying the independent
  readings, so that is precisely the wrong thing to decay.
- `round_width: 4` is a fourth, found only when this work was merged against
  the concurrent-rounds change that landed alongside it. It costs *depth*
  rather than accuracy, and the library's own reasoning is what condemns it:
  `DEFAULT_ROUND_WIDTH` says widening a **blind** round is free — "a blind
  member could not read that row anyway" — and then fixes the free width at
  four. The free width is the size of the room. A room of 128 spends 32 rounds
  completing an opening round that could take one.

`EpisodePolicy::for_room(members)` derives all four. The budget and threshold
arithmetic is not new — the benchmark has scaled both host-side since the size
sweep, and the reasoning behind each bound was recorded then. What is new is
that the derivation now lives where a host will find it instead of in an
example's `policy.rs`.

The round is widened to the whole room, with `revealed_width` left at one —
the shipping default's own rule, *take all of the free concurrency and none of
the paid kind*, applied by a constructor that knows the room's size. Measured on
the hidden profile:

```text
rounds/ep            8       32       64      128  members
hive+              9.3     33.1     65.1    135.4
hive+blind         2.3      2.1      2.1      2.6
```

Same accuracy in every column — the arms are identical on the `correct %` table
— and 52x less depth at 128 members. Depth is what a host with async seats
actually waits for.

The window is the other addition. `for_room` sets `quorum.window` to the budget,
so support deposited in the opening round still counts when the room settles;
an absolute window silently stops covering the episode somewhere above thirty
turns, and the benchmark's own fixed `window: 100` does not cover a 768-turn
episode at 256 members.

**Not done: an error.** A first pass rejected `quorum.threshold > members` at
step time as a configuration error. It is not one — a desk whose other members
retire mid-episode arrives there legitimately, and refusing to step at all is a
worse answer than reporting that the room cannot decide. The check was removed
before it shipped.

## 2. Exhaustion was undiagnosable

`HiveStep::Exhausted { spent }` carried a turn count and nothing else. A room
that spent thirty turns arguing two options to within one supporter of quorum
and a room that spent thirty turns depositing nothing returned the same thing.

That is not hypothetical. The federation collapse in the previous write-up was
exactly the second case — every desk spending its whole budget asking peers
questions, standings empty on all of them — and the repository's own recorded
diagnosis of it was **wrong for a year** because the step result could not tell
anyone. It took reading `Board::deliver` to find that the answering desk spends
no budget at all.

`Exhausted` now carries:

- `standings` — where every advocated topic stood when the budget ran out. Empty
  standings after a spent budget is the federated symptom, stated rather than
  inferred.
- `visibility` — `Blind` here means the room never saw itself, which is failure
  mode 1 above reporting itself.

Both are folded *before* the budget check rather than after it, which costs one
fold on the step that ends an episode.

## 3. Distance was measured in the wrong unit

A `Sequence` numbers a row in the **host's transcript**, not a row in the
episode. `step` folds a subset — above the watermark, addressed to the desk,
authored by a current member — and every row it does *not* fold still counted
against `QuorumPolicy::window` and against salience decay. An aside, an
off-floor exchange row, a row from a retired agent, a reading published from
another channel: all of it shortened the window of a desk that was merely busy.

`src/exchange/README.md` had stated this limit exactly and asked for the fix by
name: "Measuring decay and window in desk-visible rows instead of raw sequences
would remove the caveat, and is a change to the quorum and salience folds with
its own ADR." That is [ADR 0016](../adr/0016-distance-is-measured-in-the-rows-a-fold-reads.md).

`Horizon` now carries both the sequence a fold is measured to and the ruler it
measures with, and `EpisodePolicy::distance` selects `Basis::Sequence` (raw, the
default) or `Basis::Live` (folded rows). Public entry points take
`impl Into<Horizon<'_>>`, so a caller passing a `Sequence` is unchanged.

**Negative result: it changes nothing measurable here, and it ships off.** Run
`--distance live` against the arm that appends the most foreign rows — the
digest arm below, which puts `D - 1` of them on every journal — and the output
is byte-identical, because a window of 100 against an episode of about fifty
folded rows is not binding. That is a fact about this harness rather than a
general one, and it is the reason `Basis::Sequence` stays the default: the
convention here is that a mechanism ships off until an arm has scored it, and
this one has not been scored anywhere it bites.

**It is also not what makes concurrent and sequential federations agree.** The
previous write-up gated the concurrent scheduler behind `--jobs > 1` because it
decides differently — `swarm 78.0` against `swarm 67.0`. The reason is that a
routed referral is *delivered* one pass later, which is a genuinely different
transcript order, not a numbering artifact. No choice of ruler fixes it, and the
two schedulers remain two schedules.

Concurrency in this repository now has two widths, and it is worth being exact
about which one this is. **Within a desk** the library decides: since
[ADR 0014](../adr/0014-a-round-authorizes-concurrent-turns.md) a step authorizes
a bounded *round* of turns that may run at once, on the argument that members
writing simultaneously cannot read each other — so a concurrent round is a blind
round. **Across desks** the host decides, and that is what the scheduler here
and `--jobs` are: separate episodes, separate journals, separate budgets, and
only the waiting overlapped. `--jobs` bounds desks in flight, not calls — a
worker fills one desk's whole round in order — so widening a round shortens a
pass rather than putting more calls in the air. Neither width is the other's
business: what this work did *not* do is widen a round, and what ADR 0014 did
*not* do is make two host schedules agree.

## 4. The room-size hot loops were quadratic

Four sites, each folded once per room instead of once per member:

| Site | Was | Now |
| --- | --- | --- |
| `attention::addresses` | rebuilt the author's own sequence set and scanned it per citation, per member | one pass over the traces answers it for everybody |
| `episode::active_members` | `Roster::active_member` — itself a scan — once per desk member | the active set collected once |
| `episode::charged` | searched the carried thresholds per member | indexed once |
| threshold membership check | linear scan per carried record | set lookup |

Measured at `--jobs 1`, same machine, same seeds, with the four fixes reverted
in place and restored:

```text
members      before      after
64          63.1 µs    56.6 µs    -10%
128        198.4 µs   156.8 µs    -21%
256        934.5 µs   552.1 µs    -41%
```

The gap widens with the room, which is what a quadratic term being removed looks
like. It does not change the conclusion the previous write-up reached — at 256
members the library is 3% of wall clock, and a hundred desks of ten is a better
shape than one room of a thousand whatever the constant is.

`DeskSet::resolve_id` revalidating the whole desk set on every call, which
`step` pays three times a turn, is **not** fixed. It cannot be without either
changing the public contract that an invalid set errors, or adding a validated
handle type. It is recorded as a known cost rather than worked around.

## 5. A bounded question cannot fix a shared blind spot

The previous write-up recorded, as a limit rather than a finding, that at a
hundred desks sharing seven blind spots "everything bounded fails". That limit
turns out to have a mechanism and a fix.

The mechanism: the peer a desk asks is drawn from a federation that shares its
blind spot, so the wider the federation, the likelier the answer confirms the
error. `swarm°` at `--ask-cap 2`, ten members a desk, eight options:

```text
                                  25       50      100  desks
                                 250      500     1000  agents
swarm°  (ask two peers)         95.0     58.3      0.0
swarm◦  (ask two, publish once) 62.5     62.5     62.5
vote    (matched budget)        15.0     20.8     12.5
pooled  (free information)     100.0    100.0    100.0
turns/ep swarm°                356.9    683.9   1339.3
turns/ep swarm◦                352.6    678.7   1332.3
digests/ep                      25.0     50.0    100.0
```

n = 40 at twenty-five desks and 24 at fifty and a hundred, every arm deciding
the same federations.

`--digest N` is the fix: each desk publishes its reading of the whole slate,
once, to *every* channel. **One model call per desk**, however large the
federation, against the `D - 1` questions and `D - 1` answers a desk would spend
asking everybody. The row lands on each peer authored by a member of the
publishing desk, and `step` folds a trace only from a current member of the desk
it is folding — so a digest deposits information into every reader and support
into nobody's standings, which is the same guarantee a referral's `!evidence`
carries, reached from the other side.

**Read it as a crossover, not a win.** Publishing is flat in the size of the
federation because every desk hears every desk either way; asking degrades
because the peer pool degrades. Below fifty desks the bounded ask is better and
the digest costs a third of the accuracy; from fifty up the digest is the only
bounded arm still standing, and at a hundred it is the difference between 62.5%
and nothing — `swarm°` there does not even agree with itself, deciding eleven of
twenty-four federations against the digest arm's twenty-four.

Turns are within 1% throughout, so what is bought is not paid for in floor time.

It does not reach `pooled`, and the gap is the point. `pooled` hands over every
member's *private facts*; a digest hands over a desk's *scored reading*, and
averaging correlated readings imports the correlation.

**Where blind spots are distinct it is a straight loss**, and measured rather
than assumed: at twenty-five desks with 128 options, `swarm° 100.0` against
`swarm◦ 55.0` over twenty federations. Publishing everybody's reading to
everybody is not a free improvement on asking one peer — it is the right move
only when the peer you would have asked is no better informed than you are.

**What it costs is rows, not calls.** One model call per desk, but `D - 1` row
appends each, every one carrying the whole slate — so the transcript grows as
`D² x topics` while the spend grows as `D`. At a hundred desks and eight options
that is ten thousand short rows and costs nothing noticeable; at a hundred desks
and 128 options it is ten thousand long ones and the harness slows down
markedly. A host with a wide slate should publish the options in contention
rather than the whole slate.

`--digest` is off by default and every recorded number was taken with it off.

## 6. The width guidance was in the wrong place

`ReferralPolicy` deliberately omits a width bound and says so: "bounding its
width is the host's job, because only the host knows what a question costs it."
That position is right and it was also a trap — the obvious host bound, *every
peer once*, is not a bound, and following it collapsed the federation from 100%
to 0.0%.

The number now sits with the doc that sends the host off to find it, in
`ReferralPolicy`'s rustdoc and in `src/referral/README.md`, with the two rules
that follow: bound width by a constant the host states rather than by the number
of peers, and do not spend an authorized turn on the question. Neither is a code
change.

## What this does not show

- **Simulated members throughout.** What is measured is whether a schedule moves
  information, not whether a model writes a useful row. The digest arm in
  particular has no live implementation: `SwarmMember::publish` returns `None`
  by default and every live seat takes that default, because publishing is a
  prompt of its own and no live arm has been written for it.
- **`Basis::Live` is untested where it would matter.** It is correct by
  construction and unmeasured in effect, which is exactly why it is off.
- **The digest's 62.5% is not obviously a ceiling.** One round was measured;
  `--digest 2` and later rounds were not, and neither was publishing facts
  rather than readings, which is what the gap to `pooled` suggests is the real
  lever.
