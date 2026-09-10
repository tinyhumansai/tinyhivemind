# A thousand agents, and the one thing that breaks first

**Date** 2026-09-10
**Status** Recorded
**Code** `cargo run --release -p tinyhivemind-hive --example bench -- --swarm`
on `hive-at-scale`, with `--desks N --per-desk 10 --topics 128 --episodes 20`.
**Sample** 20 seeded federations per size out to fifty desks and 8 at a
hundred, every arm deciding the same federations. Simulated participants, so
the numbers reproduce from the seed. The sample shrinks at the largest size
because the arm being refuted is the expensive one: `swarm` spends 6624 agent
turns per federation there, against `swarm°`'s 1342.
**Machine** 28 cores. Every run at `--jobs 28`; `--jobs` changes wall clock and
nothing else, which is asserted rather than assumed.

## The question

[The size sweep](2026-09-09-topology-at-scale.md) took one room to sixty-four
members and a federation to twenty-four desks, and found that the federation
"decides nothing at all" past twelve. This asks what happens at **a thousand
agents**, whether the cause is what that write-up said it was, and whether it
can be fixed.

It is not what that write-up said it was. The correction is recorded at the head
of the relevant section there, and repeated here because the fix follows from
it.

## What actually collapses

The recorded diagnosis blames the answering desk: "a referral costs the
*answering* desk a floor turn." The code disagrees. `Board::deliver` appends an
answer without calling `step`, so the answering desk spends no budget at all.

What costs a turn is the **ask**. `Board::take_turn` lets a member spend the
turn its episode authorized on asking another channel, and the host's width
bound permits one ask per peer — `D - 1` of them — against a turn budget of
`3 x per_desk`. The two quantities are unrelated: the number of peers grows with
the federation while a desk's budget stays whatever its own size earned.

At ten members a desk has thirty turns. At twenty-five desks it may spend
twenty-four of them asking. At fifty, it may spend forty-nine turns it does not
have, and the arithmetic is the whole finding:

```text
desks              12       25       50      100
peers to ask       11       24       49       99
turns available    30       30       30       30
```

The observed endings say exactly this. At twenty-five desks and beyond, **every
desk episode ends `exhausted`** — five hundred of five hundred, then a thousand
of a thousand, then two thousand of two thousand. Not deadlocked, not idle:
spent. The federation talks itself to death.

## The fix, and where it belongs

Not in the library. `ReferralPolicy`'s own documentation already says so:

> Note what is deliberately absent: a bound on how *many* channels one desk may
> ask. `max_hops` bounds the depth of a chain, and bounding its width is the
> host's job, because only the host knows what a question costs it.

That position is right, and the defect is the host taking the job and doing it
badly — bounding width at "every peer, once" with no reference to the budget the
asking spends. So `crates/tinyhivemind-core` and `crates/tinyhivemind-hive` are
**unchanged by this work**.

The harness gains `AskChannel`, which says where a desk pays for a question:

- `OnFloor` — a member spends its authorized turn asking, and may ask every
  peer. The arm `swarm`, and what every recorded number was taken against.
- `OffFloor { cap }` — the desk asks without taking a turn, at most `cap` times
  however many peers exist. The arm `swarm°`, at `--ask-cap 2`.

This is [ADR 0012](../adr/0012-an-exchange-round-spends-model-calls-not-turns.md)
one level up, and the argument is the same one: what an exchange spends is model
calls, not turns, and a bound that grows with the number of peers is not a bound
on cost. The ask row carries no trace marker, so the episode's own fold reads
nothing out of it — the same safety property off-floor exchange rests on, with
the same stated caveat about sequence numbering. Asks are priced in their own
`asks/ep` column rather than folded into `turns`, so the arm cannot look cheap
by hiding what it spent.

## The result

Ten members per desk, 128 options, twenty federations per size.

```text
                    3       12       25       50      100  desks
                   30      120      250      500     1000  agents
siloed            0.0      0.0      0.0      0.0      0.0
swarm            75.0    100.0      0.0      0.0      0.0
swarm°           75.0    100.0    100.0    100.0    100.0
pooled           85.0    100.0    100.0    100.0    100.0
vote              0.0     95.0    100.0    100.0    100.0
merged           10.0     80.0      0.0      0.0      0.0
```

```text
turns/ep            3       12       25       50      100  desks
swarm            47.6    510.2   1868.0   3624.0   6624.0
swarm°           47.1    186.8    356.8    685.0   1342.5
asks/ep           6.0     24.0     50.0    100.0    200.0
```

Three things fall out.

**The collapse is total and the fix is total.** `swarm` goes to zero at
twenty-five desks and stays there; `swarm°` holds 100% at every size out to a
thousand agents, and every desk converges rather than exhausting. The
information was never lost — `pooled` is at 100% throughout, and `pooled` is
just "every desk's reading already in every member's hands." What failed was
only the mechanism for moving it.

**It is cheaper, not more expensive.** At a hundred desks `swarm°` spends 1342
turns against `swarm`'s 6624, and 398 crossings against 5856. Bounding the width
of the exchange does not trade accuracy for cost; at scale it buys both, because
the turns the on-floor arm spends asking are turns it needed for deciding.

**Two asks is enough, and that is a property of the task.** At the default bias
one outside reading already overturns a desk's decoy, so a second is redundancy
rather than reach. `--ask-cap` exists because the right width depends on the
problem, not because two is a finding.

## What this does not show

- **It does not beat the control everywhere.** At twenty-five desks and beyond
  the matched-budget federated vote also reaches 100%: pooling 250 independent
  readings across desks with distinct blind spots is enough on its own. The
  claim here is narrower and worth stating plainly — the *protocol* survives at
  a scale where it previously scored zero. Where it does separate from the
  control is the correlated regime below.
- **Correlated desks are a different task, and a harder one.** The table above
  gives each desk a blind spot of its own, which needs more options than desks.
  Narrow the slate so desks share blind spots and the picture changes: at
  twenty-five desks with eight options, `swarm° 95.0` against `vote 10.0` and
  `swarm 0.0` — the protocol's best showing against the control. At a hundred
  desks with eight options, a hundred desks share seven blind spots and
  everything bounded fails: `swarm° 0.0`, `vote 10.0`, and only `pooled` at
  100.0. No bounded pairwise exchange recovers an error a hundred desks share.
  The harness now says so out loud rather than leaving the invariant quietly
  broken — a federation with more desks than non-truth options prints a note
  that some desks are wrong about the same thing.
- **Simulated members.** The participants are arithmetic. What is measured is
  whether a schedule moves information, not whether a model writes a useful row.
- **Nothing here was run live.** The concurrent scheduler that a live run needs
  is implemented and tested; a live federation of a hundred desks has not been
  run against a real endpoint.

## What else had to change to ask the question at all

The harness refused these sizes, and mostly refused them silently.

- `--agents` clamped at 256 and `--topics` at **eight**, the length of a fixed
  name table. Both now generate past their named entries, keeping the first
  eight names so every recorded number reproduces byte for byte. The topic cap
  mattered most: a thousand members choosing between four options is a task a
  plurality solves by itself.
- `--desks` clamped at 64, now 256.
- A generated desk label was `Desk 12`, with a space — and the swarm wire format
  identifies a desk by the single word before `reads`. Every reading crossing a
  generated desk was silently dropped, so the wire protocol was quietly broken
  above four desks. Labels are now whitespace-free.
- `--sizes` clamped without deduplicating, so two sizes above the cap became two
  identical columns and a `find` that returned the first — a column labelled
  with a size the harness never ran.
- An unrecognised flag was ignored, so `--scale-sweeps` ran the default
  comparison and reported it under the heading the operator thought they had
  asked for. Unknown flags are now refused.

And it was too slow. The per-room and per-federation loops now spread across
cores (`--jobs`, default one per core), which took the recorded five-thousand-room
comparison from 6.2 s to 0.47 s and the six-size scale sweep from 10.6 s to
0.94 s. Rooms are independent and seeded individually, and results are folded in
**room order** whatever order the threads finish in — `Aggregate::correct_flags`
is positional and the paired bootstrap resamples two arms index-for-index, so
folding out of order would leave the accuracy column right and every confidence
interval quietly wrong. Every number is identical at `--jobs 1` and `--jobs 28`
except `ns/step` and `episodes/s`, which are per-thread wall-clock readings and
legitimately move.

## The cost of a large room, measured

Worth recording because it reorders what to optimise. At 256 members in one
room, the library is **3% of wall clock** — 2.68 s of 89.7 s across every arm.
The rest is the harness re-folding each turn's projection. The library's own
`ns/step` does grow steeply with the room (61.9 µs at 64 members, 169.6 µs at
128, 872.7 µs at 256), and there are known quadratic sites behind that —
`DeskSet::resolve_id` revalidates the whole desk set on every call and `step`
pays it three times a turn — but a host reading this should note that a single
room of a thousand is not the shape a hive mind at scale takes. A hundred desks
of ten is, and it costs a fraction of the same headcount in one room.

At 256 members the tuned policy also asks for a quorum of 129 within a budget of
768 turns, and every episode ends `exhausted`. The room size at which one desk
stops being able to decide anything is well below the size at which it stops
being able to run.
