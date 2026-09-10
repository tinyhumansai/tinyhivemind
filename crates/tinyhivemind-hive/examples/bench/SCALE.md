# Running the benchmark at scale

Companion to [`README.md`](README.md): what changes when the room is a thousand
members or the federation is a hundred desks, and which knobs decide what it
costs. The findings are in
[the scale write-up](../../../../docs/experiments/2026-09-10-hive-at-scale.md).

## `--jobs`: spreading the sample across cores

Rooms and federations are independent and individually seeded, so the per-sample
loops in the comparison, the scale sweep, the context sweep and the federated
comparison all run across cores. `--jobs` defaults to one thread per core.

```sh
cargo run --release -p tinyhivemind-hive --example bench -- --episodes 5000
cargo run --release -p tinyhivemind-hive --example bench -- --episodes 5000 --jobs 1
```

Those two print the same bytes, and that is the contract rather than an
aspiration. Results are folded in **sample order** whatever order the threads
finish in, because `Aggregate::correct_flags` is positional and the paired
bootstrap under the second table resamples two arms index-for-index — they
decided the *same* rooms. Fold out of order and the accuracy column stays right
while every confidence interval goes quietly wrong.

Two columns legitimately move: `ns/step` and `episodes/s`. Both are per-thread
wall-clock readings summed across threads, so under contention they rise. **Take
the library's own cost at `--jobs 1`**; at higher job counts they measure the
machine as much as the code.

Measured on 28 cores: the 5000-room comparison goes from 6.2 s to 0.47 s, and
`--scale-sweep` over six sizes from 10.6 s to 0.94 s.

## How big a room, and how wide a slate

| knob | ceiling | note |
| --- | --- | --- |
| `--agents` | 1024 | a spending limit, not a library one |
| `--topics` | 256 | ids are generated past the eight named options |
| `--desks` | 256 | |
| `--per-desk` | 1024 | |

The first eight member names, desk names and option ids are the ones every
recorded number was written against, so those numbers reproduce exactly rather
than approximately; past them all three generate.

**Grow the slate with the room.** A thousand members choosing between four
options is a task a plurality solves by itself, and a benchmark run there
measures the law of large numbers rather than the library — which is the same
warning `scale.rs` opens with. `--topics` was capped at eight by the length of a
name table until this work; it no longer is, and at large sizes it should move
with `--agents`.

A size above a ceiling is clamped **and says so**, and repeated sizes are
deduplicated rather than run twice under two labels. An unrecognised flag is
refused rather than ignored: `--scale-sweeps` used to run the default
comparison and report it under the heading you thought you had asked for.

## `--distance`: what a window counts

| value | the window and the decay count |
| --- | --- |
| `sequence` | every row the host wrote, folded or not (the default) |
| `live` | only the rows this episode folds |

A sequence numbers a row in the host's transcript, not a row in the episode. An
aside, an off-floor question, a row from a retired agent and a reading published
from another channel all consume one, and under `sequence` all of them count
against `QuorumPolicy::window` and against salience decay — so a desk that is
merely busy has a shorter window than an idle one on the same policy.

The two are identical on a journal the episode owns end to end, which is what
this harness mostly builds and why no recorded number moves. See
[ADR 0016](../../../../docs/adr/0016-distance-is-measured-in-the-rows-a-fold-reads.md).

## A federation is the shape that scales

A thousand agents in one room and a thousand across a hundred desks are not the
same experiment, and only the second is cheap. At 256 members in one room the
library is 3% of wall clock and the tuned policy asks for a quorum of 129 within
768 turns, so every episode ends `exhausted`: the size at which one desk stops
being able to *decide* is well below the size at which it stops being able to
run.

```sh
cargo run --release -p tinyhivemind-hive --example bench -- \
  --swarm --desks 100 --per-desk 10 --topics 128 --episodes 20
```

## `--ask-cap`: what a question costs the desk that asks it

The knob that decides whether a large federation works at all.

| value | channel | behaviour |
| --- | --- | --- |
| `0` | on the floor | a member spends its authorized turn asking, and may ask every peer once |
| `N > 0` | off the floor | the desk asks without taking a turn, at most `N` times (default 2) |

On the floor, the number of peers grows with the federation while a desk's turn
budget stays whatever its own size earned. At twenty-five desks of ten a desk
may spend twenty-four of its thirty turns asking; at fifty it may spend
forty-nine turns it does not have. Every desk ends `exhausted` and the
federation decides nothing.

The `swarm°` arm asks off the floor instead, and holds 100% out to a thousand
agents while spending a fifth of the turns. Asks are priced in their own
`asks/ep` column rather than folded into `turns`, so the arm cannot look cheap
by hiding what it spent.

This is a **host** knob and deliberately not a library one: `ReferralPolicy`
bounds a chain's depth and says in its own documentation that bounding its width
is the host's job, "because only the host knows what a question costs it".
`crates/tinyhivemind-core` and `crates/tinyhivemind-hive` are unchanged by any
of the above.

## Correlated desks, and `--digest`

A federation gives each desk a blind spot of its own, which needs more options
than desks. With fewer, some desks are necessarily wrong about the same option —
a materially different and much harder task, since pooling across two desks that
share an error imports it instead of cancelling it. The harness prints a note
when that is the arrangement it built rather than leaving the invariant quietly
false, and any number read off such a run should be read as measuring that task.

**A bounded question has a ceiling here, and it is not a small one.** The peer a
desk asks is drawn from a federation that shares its blind spot, so the wider
the federation the likelier the answer confirms the error. Measured at ten
members a desk and eight options, `swarm°` at `--ask-cap 2` degrades from 95%
correct at twenty-five desks to 58% at fifty and **0% at a hundred** — where it
also stops agreeing with itself, deciding only eleven of twenty-four federations.

`--digest N` is the other mechanism. Each desk publishes its reading of the
whole slate, once, to *every* channel: one model call per desk however large the
federation, against the `D - 1` questions and `D - 1` answers a desk would spend
asking everybody. The row lands on each peer authored by a member of the
publishing desk, and `episode::step` folds a trace only from a current member of
the desk it is folding — so a digest deposits information into every reader and
support into nobody's standings, which is the same guarantee a referral's
`!evidence` carries. It is priced in its own `digests` column.

The `swarm◦` arm is `swarm°` plus `--digest 1`:

```text
8 options, 10 members a desk        25       50      100  desks
swarm°  (ask two peers)           95.0     58.3      0.0
swarm◦  (ask two, publish once)   62.5     62.5     62.5
vote    (matched budget)          15.0     20.8     12.5
pooled  (free information)       100.0    100.0    100.0
turns/ep swarm°                  356.9    683.9   1339.3
turns/ep swarm◦                  352.6    678.7   1332.3
digests/ep                        25.0     50.0    100.0
```

Read it as a **crossover, not a win**. Publishing is flat in the size of the
federation because every desk hears every desk either way; asking degrades
because the peer pool degrades. Below fifty desks the bounded ask is better and
the digest is a real cost in accuracy; from fifty up the digest is the only
bounded arm still standing. Turns are within 1.3% of each other throughout —
356.9 against 352.6 at twenty-five desks is the widest gap — so what is being
bought is not paid for in floor time.

It does not reach `pooled`, and the gap is the point: `pooled` hands over every
member's *private facts*, while a digest hands over a desk's *scored reading*.
Averaging correlated readings imports the correlation. Where every desk's blind
spot is its own — the `--topics 128` arrangement above — it is a straight loss:
at twenty-five desks with 128 options, `swarm° 100.0` against `swarm◦ 55.0`.

**What it costs is rows, not calls.** One model call per desk, but `D - 1` row
appends each, every one carrying the whole slate — so the transcript grows as
`D² x topics` while the spend grows as `D`. At a hundred desks and eight options
that is ten thousand short rows and unnoticeable; at a hundred desks and 128
options it is ten thousand long ones and the run slows down markedly. A host
with a wide slate should publish a digest of the options in contention rather
than of all of them.

`--digest` is off by default, and every recorded number was taken with it off.

## Driving it live

`--jobs` also decides how many live desks call a model at once, and there it
selects a genuinely different scheduler — see
[`LIVE.md`](LIVE.md#a-federation-of-live-desks-running-at-once), which also
covers pointing the harness at a lightweight OpenHuman build.
