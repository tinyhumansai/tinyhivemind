# Architecture decision records

One file per significant decision, numbered in the order it was taken. An ADR
records *why* a boundary sits where it does, so the reasoning survives the
people who had it and the decision is not silently reversed by whoever touches
the code next.

An accepted ADR is **immutable**. It is never edited to reflect a change of
mind; it is superseded or amended by a later record that links back to it. A
record still marked Proposed is live — the decision is argued but not closed,
and the mechanism it describes usually ships off by default until it is.

## Writing one

Copy the shape of
[`0001-record-architecture-decisions.md`](0001-record-architecture-decisions.md).
The filename is `NNNN-a-sentence-in-the-imperative.md`, zero-padded to four
digits, and the heading repeats that sentence. Every record opens with a
**Status** (Proposed, Accepted, or Accepted-and-superseded) and a **Date**, and
carries **Context**, **Decision**, and **Consequences** sections. Link the
specification in [`../specs/`](../specs/README.md) that the decision serves, and
link any earlier ADR it amends.

## The record

| # | decision | status |
| --- | --- | --- |
| [0001](0001-record-architecture-decisions.md) | Record architecture decisions | Accepted |
| [0002](0002-hive-episodes-are-sequential.md) | Hive episodes are sequential, and visibility is the fan-out knob | Accepted |
| [0003](0003-refutation-links-evidence-to-a-topic.md) | Refutation links evidence to a topic, and caps rather than debits | Accepted, ships off by default |
| [0004](0004-grounds-are-weighed-by-evidential-depth.md) | Grounds are weighed by evidential depth, not counted | Accepted, ships off by default |
| [0005](0005-a-blind-round-may-be-concurrent.md) | A blind round may be concurrent | Proposed — amends 0002 |
| [0006](0006-a-referral-crosses-one-channel-at-a-time.md) | A referral crosses one channel at a time, and carries information rather than a vote | Accepted |
| [0007](0007-the-directory-is-folded-from-citations.md) | The directory is folded from citations, and the host's affinity is a prior rather than an authority | Accepted |
| [0008](0008-an-approval-decision-is-total.md) | An approval decision is total: it denies rather than fails | Accepted |
| [0009](0009-a-refusal-renders-what-the-caller-already-holds.md) | A refusal renders only what the caller already holds | Accepted |
| [0010](0010-an-aside-carries-information-never-support.md) | An aside carries information rather than support, and a redaction is a row rather than an absence | Proposed |
| [0011](0011-an-aside-rides-alongside-a-turn.md) | An aside rides alongside the turn that authored it rather than spending one | Proposed — amends 0010 |
| [0012](0012-an-exchange-round-spends-model-calls-not-turns.md) | An exchange round spends model calls rather than turns | Proposed — follows 0011 |
| [0013](0013-a-vendored-crate-is-an-example-dependency.md) | A vendored crate may back an example and never a library crate | Accepted |

## Reading order

[0002](0002-hive-episodes-are-sequential.md) is the one to read first: it fixes
*one message, one turn* as a type invariant, and every later record about the
hive either lives inside that constraint or says explicitly how it amends it.
[0010](0010-an-aside-carries-information-never-support.md) →
[0011](0011-an-aside-rides-alongside-a-turn.md) →
[0012](0012-an-exchange-round-spends-model-calls-not-turns.md) form one
argument about what a private row is and what it costs, and should be read as a
sequence. The measurements that pushed 0011 and 0012 are in
[`../experiments/`](../experiments/README.md).
