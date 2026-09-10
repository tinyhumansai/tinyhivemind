# 14. Distance is measured in the rows a fold reads

- **Status:** Proposed
- **Date:** 2026-09-10

## Context

Three folds in `tinyhivemind-hive` are *local*, and all three ask the same
question — how far back is this trace from where the room is now.
`QuorumPolicy::window` bounds which support still counts, `SalienceWeights::
half_life` decays the recency term, and `DirectoryPolicy` does both. Until now
all three answered it by subtracting raw `Sequence` values.

A sequence numbers a row in the **host's transcript**, not a row in the
episode. `episode::step` folds a subset: rows above the watermark, addressed to
the desk, authored by a current member. Everything else still consumes a
sequence number and still counts against the window and the decay:

- the conversation that preceded the episode, when the watermark is not zero;
- a private aside, which [ADR 0010](0010-an-aside-carries-information-never-support.md)
  drops from the fold entirely;
- an off-floor exchange row, which [ADR 0012](0012-an-exchange-round-spends-model-calls-not-turns.md)
  makes it a host's business to write many of;
- a row from a retired agent;
- in a federation, a reading published from another channel.

So two desks on the same policy, deciding the same question, get different
windows because one of them is busy in ways its episode is not reading. Nothing
reports it. The room simply decides less well.

This was already known and already written down. `src/exchange/README.md` states
the limit exactly — "the **fold** ignores private rows completely, and the
**sequence numbering** does not" — and names the fix: "Measuring decay and
window in desk-visible rows instead of raw sequences would remove the caveat,
and is a change to the quorum and salience folds with its own ADR." This is that
ADR.

The scale work is what made it worth doing rather than worth noting. A
federation that publishes one reading per desk appends `D - 1` foreign rows to
every journal, which under raw arithmetic is `D - 1` of every desk's window
spent on rows that desk's episode does not fold.

## Decision

**A fold measures distance in the rows it reads, when the host asks it to.**

`Horizon` carries the sequence a fold is measured to *and* the ruler it
measures with, and it is what `quorum::standings`, `salience::salience`,
`directory::directory` and `attention::BidContext` now take where they took a
bare `Sequence`. `EpisodePolicy::distance` selects between:

- `Basis::Sequence` — subtract raw sequence values. The original behaviour.
- `Basis::Live` — count the rows the fold reads.

The two are **identical on a dense journal the episode owns end to end**, which
is why every recorded benchmark number is unchanged. They diverge exactly where
a real host differs from the harness.

`Basis::Sequence` stays the default. Two mechanisms in this crate have already
been measured, lost, and been left opt-in rather than quietly defaulted on; this
one has not yet been scored on an arm where it makes a difference, and it does
not get a shortcut past that bar for being obviously more correct.

## Consequences

- The caveat in `src/exchange/README.md` and in `AskChannel::OffFloor`'s
  documentation is now removable by configuration rather than only measurable.
  A host writing many off-floor rows per turn sets `Basis::Live` and stops
  having to reason about the interaction.
- `Horizon` is taken as `impl Into<Horizon<'_>>` at every public entry point, so
  a caller passing a `Sequence` compiles and behaves unchanged. `BidContext.at`
  is a field and is the one place a caller must write `.into()`.
- An unfolded sequence still has an answer: it measures to the position it would
  occupy among the folded rows. That keeps `distance` monotone and total rather
  than introducing a case a caller has to handle.
- **It is not what makes a concurrent and a sequential federation schedule agree.**
  Those differ because a routed referral is *delivered* a pass later, which is a
  genuinely different transcript order rather than a numbering artifact. No
  choice of ruler fixes that, and `examples/bench/swarm/schedule.rs` documents
  it as the separate thing it is.
- The benchmark's `--distance live` runs the federated arms both ways. On the
  digest arm — the one that appends foreign rows to every journal — it makes no
  difference at the windows this harness runs, because a window of 100 against
  an episode of about fifty rows is not binding. That is a measurement of this
  harness, not a general result, and it is the reason the basis ships off.

## Alternatives considered

**Renumber the traces to their rank before folding.** Confines the change to
`episode::step` and leaves every fold's arithmetic untouched. Rejected: a trace
carries `target` and `cites` as sequences, so renumbering means remapping
citation identity as well as distance, and a citation to a row the fold never
read has no rank to be remapped to. Measuring distance differently is a strictly
smaller change than renaming the rows.

**Make `Basis::Live` the default.** Rejected for now, on the convention above
rather than on doubt about which is more correct. It is a candidate for the
default once an arm scores it somewhere it is binding.
