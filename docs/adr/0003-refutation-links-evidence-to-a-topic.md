# 3. Refutation links evidence to a topic, and caps rather than debits

- **Status:** Accepted, shipped **off by default**
- **Date:** 2026-09-01

## Context

`crates/tinyhivemind-hive` gives a room one negative move. `!object >N ^M`
names a *message*, and cross-inhibition removes that message's author from the
supporter set of every topic that message advocated. [ADR 0002](0002-hive-episodes-are-sequential.md)
and [`../specs/hive-mind.md`](../specs/hive-mind.md) record why it targets the
advocate: subtracting from a score cannot break a tie between two equally
supported options, and silencing an advocate can.

That is right, and it is half of the mechanism it was taken from.

The [live hidden-profile run](../experiments/2026-09-01-live-hidden-profile.md)
found the other half by failing without it. In every failed room — four of the
paired ten, and both earlier failures — scout's refutation was **already in the
transcript** at a sequence every later message could cite: the retry path
shipped disabled, zero retries fired today. It changed nothing. Members went on
depositing `!support #retries ^6`, and three grounded supporters carried the
decoy.

The grammar gives a member no way to say *this evidence refutes that topic*.
Killing a hypothesis with a fact means objecting to each advocate separately,
one turn each, and a five-member room at a budget of fifteen runs out of budget
before it runs out of advocates. The count is the whole argument: refuting a
topic with `k` advocates costs `k` turns, and each new supporter costs one more.

### What the biology actually says

The model this crate's cross-inhibition is taken from is Pais, Hogan, Schlegel,
Franks, Leonard & Marshall, "A mechanism for value-sensitive decision-making",
*PLoS ONE* 8(9):e73216 (2013):

    dx_A/dt = α_A(v_A)·x_u − ρ_A·x_A − β·x_A·x_B + noise

`β` is the stop signal, and `!object` is `β`. But recruitment `α_i` is an
increasing function of **the site's value `v_i`**, and every scout derives `v_i`
by inspecting the site herself (Seeley & Visscher, *J. Exp. Biol.*
211:3691–3697, 2008 — dance length is proportional to the scout's own absolute
assessment). Evidence bearing on a site does not silence its advocates; it
changes what every scout would independently conclude about the site, which
lowers `α` for everyone at once, including those not yet recruited.

There is no term in this crate for that. `TopicStanding` has supporters,
silenced authors and a `support` sum, and no representation of the topic's
*standing as a hypothesis* independent of who is advocating it.

## Decision

Add a fifth trace kind, `Refute`, with the grammar `!refute #topic ^N`. It
**requires both a topic and at least one citation**; a marker missing either
yields no trace, exactly as an unparsable marker does today.

A refutation contributes to `TopicStanding.refuted_by`, a set of distinct
refuting agent ids folded on the same `(sequence, offset)` key as everything
else. `QuorumPolicy` gains `refutation_cap: u32`, and `TopicStanding::carried`
returns `false` once `refuted_by.len() >= refutation_cap`, whatever the
support says.

**It caps; it does not debit.** The argument is the same one that shaped
`!object` and it is worth restating because the conclusion is the opposite
shape. A debit is a scalar subtracted from `support`, and `support` is not what
`carried` reads — `carried` reads `supporters.len()` against `threshold`. A
debit would therefore either do nothing, or would have to be converted into
"remove a supporter", which is `!object` with a worse name and no way to say
*which* supporter. A cap is the only thing that expresses *this hypothesis is
dead regardless of how many people like it*, which is what the failed rooms
needed and could not write.

`refutation_cap` is `Option<u32>`. `Some(2)` is the setting a room that wants
the mechanism should use — a room should not lose a hypothesis to one member's
assertion — and `Some(0)` is `Error::ZeroRefutationCap`, matching
`ZeroQuorumThreshold` and `ZeroQuorumWindow`.

The **default is `None`**, and that is an empirical decision rather than
caution. The benchmark arm this ADR required scored 75.0% against 82.1% for the
same policy without it, below even the matched-budget vote at 78.5%, and no policy with the cap on reached the top twelve of
an 864-point grid search. The type carries the result: `None` is "off", not an
unreachable number pretending to be a cap.

`importance(Refute)` is 850, between `Object` at 800 and `Propose` at 900. A
refutation is a stronger move than an objection because it is directed at the
hypothesis rather than at a person, and a weaker one than putting a new option
on the floor.

## Consequences

- **`!object` stays, and stays advocate-directed.** The two moves are different
  and both are needed. `!object` breaks a tie between two options that are both
  live; `!refute` kills one that is not. Nothing about cross-inhibition changes.
- **A refutation is cheap to make and bounded in effect.** One turn kills a
  hypothesis for the whole room, where before it took one turn per advocate.
  The bound against abuse is `refutation_cap` requiring distinct agents and the
  grammar requiring a citation, so a refutation is grounded by construction —
  `Trace::grounded()` is true for every `Refute` that parses.
- **A capped topic can still be discussed.** `carried` is false; the topic keeps
  its supporters, its `support` sum and its place in `standings`, so the
  transcript records that the room considered it and why it fell. Nothing is
  deleted, and a reader can audit the refutation back to the message it cites.
- **`Deadlocked` gets rarer and `Exhausted` gets *more* common.** Two tied
  topics where one is refutable resolve in one turn, and the deadlock count does
  fall — but turns per episode rise from 6.84 to 8.95 and exhaustion from 5
  episodes in 500 to 59. A narrowing rule spends budget.
- **It costs accuracy on the benchmark's task, and the cost scales with noise.**
  7 points at ±90 evaluation noise, 15 at ±120, nothing at ±30. The reading with
  the most support is that a refutation is *global* where an objection is local:
  an objection removes one advocate, a refutation caps the topic for everybody,
  so a member firing one on a noisy read removes an option for the whole room.
  The full numbers, and what the benchmark does not test, are in
  [`../experiments/2026-09-01-refutation-and-grounds.md`](../experiments/2026-09-01-refutation-and-grounds.md).
- **This does not check that the cited message refutes anything.** The library
  counts distinct grounded refuters, and as a pure fold it cannot read the
  citation and judge it — the same limit `require_grounded` already has, stated
  in [`../specs/hive-mind.md`](../specs/hive-mind.md). A room can refute a true
  hypothesis, exactly as the one live `!object` observed fired against the
  correct option. The mechanism is neutral; a benchmark that reports only its
  wins is not reporting it.
- **Reversing this is a wire-format change.** `TraceKind` is a serialized enum
  pinned by unit tests, and `TopicStanding` and `QuorumPolicy` both gain a
  field. `tinyhivemind-hive::Error` is `#[non_exhaustive]`, so the new variant
  is additive; the two payload structs are not, so the phase lands before any
  host pins them.

## Related

- [ADR 0004](0004-grounds-are-weighed-by-evidential-depth.md), which is the
  other half of "support is counted; grounds are not weighed".
- [`../specs/refutation-and-grounds.md`](../specs/refutation-and-grounds.md).
- [`../research/biology.md`](../research/biology.md), for the `α(v)` term and
  the honeybee measurements.
- [`../experiments/2026-09-01-live-hidden-profile.md`](../experiments/2026-09-01-live-hidden-profile.md),
  open item 2, which asked for this ADR.
