# Experiments

Dated records of running the library for real. An experiment answers a question
that was written down before the numbers existed, reports what happened run by
run so a small sample reads as one, and says plainly what should change because
of it — including when the answer is that the mechanism under test should not
ship.

These are not marketing. Several of the records below are negative results, and
one supersedes itself in its own opening paragraph. That is the point: an
experiment that could not have lost was not an experiment.

## Conventions

The filename is `YYYY-MM-DD-a-short-slug.md`. Each record opens with its
**Date**, a **Status** (Recorded, or Recorded and later superseded), the
**Code** that produced it — the exact example and flags — and links to the
**Spec** it tests and the **Decision** it fed. State the sample size, and mark
an estimate as an estimate. Keep every file at 500 lines or fewer; when a matrix
outgrows that, split the raw rows into a companion file and link it, as
[`2026-09-05-expert-delegation-live-matrix.md`](2026-09-05-expert-delegation-live-matrix.md)
does.

## The record

| date | question | answer |
| --- | --- | --- |
| [2026-09-01](2026-09-01-live-hidden-profile.md) | Does a live room beat a poll on a problem that has an answer? | The synthetic brief was not enough; a real problem was needed to tell them apart |
| [2026-09-01](2026-09-01-refutation-and-grounds.md) | Do refutation and evidential grounds earn their place? | The arm was able to lose, and it lost |
| [2026-09-02](2026-09-02-federated-hidden-profile.md) | Can several channels pool what only one of them knows? | Yes, across one referral hop at a time |
| [2026-09-05](2026-09-05-expert-delegation.md) | Who knows, and what is that worth? | Directory-folded expertise pays; see the [full matrix](2026-09-05-expert-delegation-live-matrix.md) |
| [2026-09-07](2026-09-07-do-asides-help.md) | Do private asides help a room decide? | No — a pairwise check is pure cost, and privacy is never the variable. Partly superseded |
| [2026-09-07](2026-09-07-private-asides.md) | What does an aside look like on a live desk? | Three agents, one desk, recorded end to end |
| [2026-09-07](2026-09-07-why-asides-lose.md) | Why did asides lose, and what is peer information actually worth? | +9 to +31 points for the information; the floor turn was what cost too much |
| [2026-09-07](2026-09-07-pe1006-desk.md) | Can a desk of real agents close one genuinely hard problem? | The harness works; the problem is not closed |
| [2026-09-09](2026-09-09-desk-lessons.md) | What should be built next after PE 1006? | Working notes and measurements from runs 21–27 |
| [2026-09-08](2026-09-08-pe1006-tool-room.md) | Does a tool-call room with a standing account beat a fenced one? | PE 1006 solved; re-reading fell to 3% of calls, but no fold ever fired |

## Reading order

[`2026-09-07-do-asides-help.md`](2026-09-07-do-asides-help.md) →
[`2026-09-07-why-asides-lose.md`](2026-09-07-why-asides-lose.md) is the clearest
worked example of the method: a mechanism is specified, measured, found to lose,
and then decomposed until the part that actually carried the value is separated
from the part that was paying for it. The decisions that followed are
[ADR 0011](../adr/0011-an-aside-rides-alongside-a-turn.md) and
[ADR 0012](../adr/0012-an-exchange-round-spends-model-calls-not-turns.md).

[`2026-09-07-pe1006-desk.md`](2026-09-07-pe1006-desk.md) →
[`2026-09-09-desk-lessons.md`](2026-09-09-desk-lessons.md) are a pair: what
happened, then what to build because of it.
[`2026-09-08-pe1006-tool-room.md`](2026-09-08-pe1006-tool-room.md) closes that
thread: the desk finally solves PE 1006, and the record separates the part of
the win that was measured from the mechanism that never ran.

The harnesses that produce these numbers are documented in
[`../../crates/tinyhivemind-hive/examples/bench/README.md`](../../crates/tinyhivemind-hive/examples/bench/README.md)
and
[`../../crates/tinyhivemind/examples/crosstalk/README.md`](../../crates/tinyhivemind/examples/crosstalk/README.md),
and the headline benchmark report lives on the wiki's
[Benchmarks](https://github.com/tinyhumansai/tinyhivemind/wiki/Benchmarks) page.
