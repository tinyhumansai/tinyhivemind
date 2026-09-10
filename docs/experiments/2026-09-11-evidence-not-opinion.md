# Evidence, not opinion

**Date** 2026-09-11
**Status** Recorded
**Answers** the open question left by
[the scale run](2026-09-10-what-the-scale-run-found.md): why `pooled` keeps
winning.
**Code** `cargo run --release -p tinyhivemind-hive --example bench -- --swarm
--desks N --per-desk 10 --topics 8 --episodes 24 --digest 1 --evidence`
**Sample** 24 federations per cell, every arm deciding the same federations.
Simulated participants, so every number reproduces from its seed.

## The question

Every bounded mechanism this benchmark has measured plateaus below `pooled`,
the arm that hands every reading to every member for free. The digest was the
clearest case: it moves information *perfectly* — one model call per desk, every
desk hearing every desk — and still stopped at **62.5%** where `pooled` reached
100.

The previous write-up guessed why, and the guess is testable: `pooled` hands
over members' **private facts**, while every bounded mechanism hands over their
**scored conclusions**, and averaging correlated conclusions imports the
correlation.

## What a federation actually held

The guess turned out to understate it. A federation held **no facts at all**.
Every member had a noisy score per option and a slant toward its own desk's
decoy; the only thing a channel could carry was an opinion. So the digest was
not losing information — there was nothing else to move.

`--evidence` changes that. It plants the disqualifying facts, each on a desk
**other than** the one that needs it, so no desk can cure its own blind spot
without crossing a channel. The two clauses then travel in one row:

```text
Payments reads #stage at 42, #ship at 118, #canary at -41. Payments rules out #canary.
```

They behave differently on the far side, and that asymmetry is the whole
finding. A reading is **averaged** into the reader's own, so peers who share a
bias reinforce it. A fact is **subtracted** — a flat `GROUNDS_WEIGHT` off that
option, whoever said it and however many peers disagree — so it cannot be
diluted, and a shared bias cannot outvote it.

It costs a clause, not a call: the same asks, the same digests, the same turns.

## The result

A hundred desks of ten — a thousand agents — eight options, so blind spots are
shared. This is the regime where every bounded arm previously failed.

```text
                                  opinions   evidence
siloed  (no channel)                   0.0        0.0
swarm   (ask on the floor)              0.0        0.0
swarm°  (ask two peers)                 0.0        0.0
swarm◦  (ask two, publish once)        62.5      100.0
pooled  (free information)            100.0      100.0
vote    (matched budget)               12.5       12.5
turns/ep swarm◦                      1332.3     1330.5
```

**The gap closes completely.** A bounded protocol — 100 model calls of digest
across a thousand agents, 1330 turns against `pooled`'s 1106 — reaches the
free-information ceiling it had never reached before.

**And reach is what decides it.** `swarm°` carries exactly the same facts and
stays at 0.0%, because a bounded ask touches two of ninety-nine peers and the
desk holding its cure is one of them about twice in ninety-nine tries. Evidence
is worth carrying only if it arrives:

```text
8 options, 10 a desk        25       50      100  desks
swarm°  opinions           95.8     58.3      0.0
swarm°  evidence          100.0     87.5      0.0
swarm◦  opinions           62.5     62.5     62.5
swarm◦  evidence          100.0    100.0    100.0
```

A bounded ask does carry a fact usefully while the federation is small enough
for two peers to be a real fraction of it. At fifty desks it reaches 87.5%; at a
hundred it reaches nothing, because two of ninety-nine is not a sample.

## The half that could have sunk it

A fact does not average. That is why it survives a shared bias — and why a
**wrong** one is more dangerous than a wrong opinion: it discounts the right
answer for every desk it reaches, undiluted. A protocol measured only on true
facts has measured the value of a channel and nothing about the risk of one.

`--fact-noise N` plants `N` per mille of the facts naming the *truth* instead.
Fifty desks:

```text
wrong facts (per mille)      0      100      250      500
swarm°  (ask two)         83.3     83.3     79.2      4.2
swarm◦  (publish)        100.0    100.0    100.0     75.0
vote                      37.5     16.7      4.2      0.0
pooled                   100.0    100.0    100.0    100.0
```

**Redundancy is what protects a fact.** Broadcasting is unharmed by a quarter
of the evidence being wrong and still reaches 75% when *half* of it is, because
a desk hearing fifty facts can afford several bad ones. The bounded ask hears
two, so one wrong fact is half of everything it knows — and it collapses from
83.3% to 4.2%.

That is the same property read twice. Broadcast makes evidence *arrive*, and
arriving many times over is also what makes it *safe*. The mechanism that
carries a fact usefully and the mechanism that survives a false one are not two
designs; they are one.

## What this does not show

- **The facts are exogenous.** They are planted, not discovered. Nothing here
  says a participant would recognise the disqualifying fact it holds, or state
  it rather than its conclusion — which is exactly what a language model has to
  do for any of this to transfer.
- **A fact is binary and a reading is a scalar.** In this sim disqualification
  is the only kind of evidence, and it applies a fixed discount. Real evidence
  is partial, and weighing it is a harder problem than applying it.
- **`vote` degrades under wrong facts and `pooled` does not**, which flatters
  the comparison. `pooled` averages every reading in the federation, so the
  bias it needs to overcome is already averaged away before a wrong fact
  arrives.
- **Simulated throughout, and never run live.** No live arm publishes anything:
  `SwarmMember::publish` returns `None` for every live seat.
- **One regime.** Eight options across ten-member desks. Where blind spots are
  distinct the bounded ask already reaches 100% and none of this is needed.

## What it changes about the earlier conclusion

The scale write-up ended by saying the protocol was "a claim about plumbing"
rather than about intelligence, and that the open question was why `pooled` kept
winning. The answer is that the plumbing was carrying the wrong cargo. A
bounded, priced protocol *can* reach the free-information ceiling at a thousand
agents — it has to carry evidence rather than opinion, and it has to broadcast
rather than ask.

That is still not a claim that a room of models reasons better than one. It is a
claim that the ceiling this benchmark kept measuring was not a ceiling on the
protocol.
