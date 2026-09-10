# Off-floor exchange

An aside that rides alongside the turn that authored it is free but rationed:
only the member the attention market authorized may write one, so a room
converging in eleven turns writes at most eleven private rows. This module lifts
that ration. Between turns — or before the first one, never *during* one — a host
may run an **exchange round** in which each member the library names appends at
most one private row, taking no floor and producing no turn.

The behavior is [`docs/specs/off-floor-exchange.md`][spec]; the decision and its
reasoning are [ADR 0012][adr].

## The property everything rests on, and its exact limit

A private row is dropped by `live_traces` before it can reach a trace, a
standing, the sequence they fold at, the directory or the floor, and
`EpisodeState::spent` counts turns rather than rows. So `step` returns the same
`HiveStep` — *including the state it commits* — **for the same desk rows at the
same sequences**.

That qualifier is load-bearing and is not a formality. A private row consumes a
sequence number, so a host allocating sequences live pushes every later desk row
up by one per row written. `QuorumPolicy::window` and salience decay both read a
*raw* sequence distance, so a support that was inside the window can fall
outside it and the episode will legitimately decide something else.
`episode::test::shifting_desk_sequences_past_a_private_row_can_change_the_step`
demonstrates it at `window: 2`.

So the honest statement is narrower than "an exchange cannot move the room": the
**fold** ignores private rows completely, and the **sequence numbering** does
not. What bounds the second in practice is that a window and a half-life are
large relative to the private rows written between two desk turns —
`QuorumPolicy::DEFAULT` uses a window of 100 against episodes of about eleven
desk turns — and the benchmark measures the residual at zero (below). A host
running tight windows, or writing many more private rows per turn than this,
should measure rather than assume. Measuring decay and window in desk-visible
rows instead of raw sequences removes the caveat, and that is now
`EpisodePolicy::distance`: set it to `Basis::Live` and the window counts the
rows the episode folds rather than every row the host wrote. It is off by
default, on the convention every opt-in mechanism here follows. See
[ADR 0016](../../../../docs/adr/0016-distance-is-measured-in-the-rows-a-fold-reads.md).

That is asserted rather than argued, and in two places, because it is the whole
safety case:

- `episode::test::an_aside_riding_along_with_a_turn_costs_the_room_nothing` — one
  readable case.
- `tests/fuzz_invariants.rs::asides_interleaved_into_an_arbitrary_transcript_do_not_move_the_episode`
  — arbitrary private rows, carrying the same fuzzed grammar as the desk rows so
  private `!propose`/`!support`/`!commit` are in the corpus, interleaved anywhere
  in an arbitrary transcript.

Both go red if the audience filter is removed. Both hold desk sequence numbers
fixed across the two transcripts they compare, which is exactly the scope of the
guarantee above — they prove the fold ignores private rows, not that live
sequence allocation is neutral. The third test named above proves it is not.

## The public surface

```rust
pub fn exchange(
    policy: &ExchangePolicy,
    state: &EpisodeState,
    opened: ExchangeState,
    transcript: &[SessionMessage],
    roster: &Roster<'_>,
    desks: &DeskSet<'_>,
) -> Result<ExchangeRound>;
```

A fold over what the caller already holds, including `opened` — the round count
the host carries and advances from each open round's `next`. See "Contacts are
folded; rounds are carried", below, for why that one cannot come out of the log. It answers whether a round is open,
which members it names, and how many rows the episode may still write. It does
no waiting and **chooses no peer**: who a member wants to contact is that
member's judgement, and the audience it writes is validated by
`tinyhivemind::aside::aside` exactly as an aside riding alongside a turn is.

`ExchangeRound::Open::members` is in desk order and contains only members that
are active on the episode's desk and still under their own contact cap. A host
may run any subset, in any order, including none.

## Contacts are folded; rounds are carried

A member's contacts are the private rows above the episode's watermark that it
authored. There is no counter, so there is nothing that could disagree with the
log — the discipline `standings` and `directory` already follow, and what keeps
this inside the charter's "no second journal" rule.

Rounds are different, and the difference is a defect this had before review
caught it. A round in which every named member declines to write leaves **no row
behind**, so the transcript cannot tell it from a round that never happened — and
those are the rounds that cost most per row, because the host paid to ask each
member and got nothing. Folding the count out of authored rows therefore lets
`RoundsSpent` arrive arbitrarily late and leaves the model-call budget unbounded,
which is the one thing `round_cap` exists to prevent.

So the host carries the round count in `ExchangeState`, exactly as it carries
`EpisodeState`, and advances it from the `next` every open round returns rather
than from a number of its own — so the two cannot drift.

## Bounds, and why they are two

```rust
pub struct ExchangePolicy {
    pub enabled: bool,
    pub contact_cap: u32,  // private rows one member may author, per episode
    pub round_cap: u32,    // rounds the episode may open at all
}
```

`round_cap` bounds rounds absolutely, and a round asks at most one row of each
**currently active** member, so an episode produces at most
`round_cap × max_active_members` private rows — where the max is over the
episode, because membership is the host's and this fold does not freeze it. A
desk that grows mid-episode has more members to ask, and a newcomer arrives with
its own untouched `contact_cap`; a host that adds members should read its
ceiling off the largest desk it will allow.

Freezing the opening roster in `ExchangeState` would make the ceiling a single
number, and was rejected: it would silently exclude a member the host
legitimately added, which is worse than a ceiling stated correctly. Each member
remains independently bounded by `contact_cap` whenever it joined.

`ExchangeRound::Open::remaining` is clamped **per member and then summed**, not
summed and then clamped. A round gives each member at most one row, so no member
can write more than `rounds_left` however much of its cap is left. The two forms
diverge exactly when spend is uneven — one member spent out and another
untouched — which is when a host sizing a batch off the field would overallocate.

`enabled: false`, `contact_cap: 0` and `round_cap: 0` all close every round and a
`Closed` round names which. Zero is a configuration a host may hold on purpose,
so unlike `defer_cap` it is not an error.

## Operational constraints

**A host must not dispatch an exchange row.** A row authored in a round is never
handed to `mention_dispatch`. That is what stops rows begetting rows: the
exchange cannot cascade, and its worst case stays the constant above.

An answer therefore arrives whenever the peer is next *asked* — on its own next
turn if the aside rode alongside one, or in the next round that names it if the
exchange is off the floor. Neither costs a turn the room would not otherwise
have spent, and neither is a turn this module starts.

**A refused audience is dropped, not published.** If the `aside` fold declines to
make a row private, the row is not written. Falling back to the desk would put a
second desk-visible contribution on one turn.

**A round does not interleave with a turn.** Run rounds between turns, so the
transcript a turn was composed from is never edited underneath it.

**The price is model calls, and it is the host's to authorize.** A round is *n*
of them. That is why the policy is off by default and its ceilings explicit, and
why the benchmark reports them in a `calls/ep` column of their own rather than
folding them into a cost figure that means something else. It counts members
*asked*, not rows written: a member that declines costs the same call as one
that answers, so counting rows would report a price below the one paid — 20
against the 45 actually spent, at the default cap.

## What it does spend

A sequence number. Rows are unique in the one shared journal, so a private row
takes the next sequence, and `salience::standing` reads recency as a raw
`at - trace.sequence` distance that feeds the attention market's choice of who
speaks next. A room writing private rows can therefore in principle decide
differently from one that does not, without a word of their content mattering.

Measured, it does not: the benchmark's `hive+quiet` and `hive+hush` arms write
the identical rows on the identical schedule and discard every answer, and both
are `+0.0 [+0.0, +0.0]` at up to forty-five model calls an episode. That makes the
sequence-distance decay harmless at these volumes rather than good design. A host
writing far more rows than that should measure rather than assume; decaying over
desk turns instead of raw sequences would be a library change with its own ADR.

[spec]: https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/specs/off-floor-exchange.md
[adr]: https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/adr/0012-an-exchange-round-spends-model-calls-not-turns.md
