# Horizon

Where a fold is measured to, and the ruler it measures back with.

Every local fold in this crate — quorum's window, salience's decay, the
directory's both — asks one question: *how far back is this trace from where the
room is now.* This module is that question's answer, and the reason it is a type
rather than a subtraction.

## The problem

A `Sequence` numbers a row in the **host's transcript**. An episode folds a
subset of it: rows above the watermark, addressed to the desk, authored by a
current member. Everything else consumes a sequence number and, under raw
subtraction, still counts against the window and the decay — the conversation
that preceded the episode, a private aside, an off-floor exchange row, a row
from a retired agent, a reading published from another channel in a federation.

So a busy desk has a quietly shorter window than an idle one, on the same
policy, deciding the same question. Nothing reports it; the room just decides
less well.

## The surface

| Item | What it is |
| --- | --- |
| `Basis::Sequence` | Subtract raw sequence values. The original behaviour, and what `EpisodePolicy::DEFAULT` carries. |
| `Basis::Live` | Count the rows the fold reads. A window of thirty means thirty folded rows however much other traffic the desk carried. |
| `Horizon::at` | A horizon over raw sequences. `From<Sequence>` builds one, so an existing caller is unchanged. |
| `Horizon::over` | A horizon over the ascending rows a fold reads. |
| `Horizon::sequence` | The sequence being measured to. |
| `Horizon::distance` | How far back a sequence is, in whichever unit. Zero at or after the horizon. |
| `Horizon::within` | At or before the horizon **and** inside a window of it. The two are separate questions. |

`quorum::standings`, `salience::salience` and `directory::directory` take
`impl Into<Horizon<'_>>`, so passing a `Sequence` compiles and behaves exactly as
before. `attention::BidContext::at` is a field and is the one place a caller
writes `.into()`.

## Operational constraints

- **The two agree on a dense journal the episode owns end to end.** That is the
  benchmark's case, and it is why no recorded number moves. They diverge exactly
  where a real host differs from the harness.
- **`rows` must be ascending.** `episode::step` sorts and deduplicates the
  folded sequences before building one, on the same grounds every other fold
  here is order-independent: the caller hands over a projection, not a promise.
- **An unfolded sequence still has an answer** — the position it would occupy
  among the rows. That keeps `distance` monotone and total instead of adding a
  case every caller has to handle. It is not a claim that such a sequence has a
  meaningful distance of its own.
- **It does not make a concurrent schedule agree with a sequential one.** Those
  differ because a routed message is *delivered* a pass later, which is a
  different transcript rather than a different numbering. See
  `examples/bench/swarm/schedule.rs`.
- **`Basis::Sequence` is the default**, on the convention every opt-in mechanism
  in this crate follows: it has not yet been scored on an arm where it is
  binding.

The decision and its alternatives are
[ADR 0014](../../../../docs/adr/0014-distance-is-measured-in-the-rows-a-fold-reads.md).
