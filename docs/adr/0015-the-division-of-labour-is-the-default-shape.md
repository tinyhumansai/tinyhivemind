# ADR 0015 — The division of labour is the default shape

**Status** Accepted · **Date** 2026-09-10 ·
**Supersedes** nothing · **Builds on**
[ADR 0014](0014-a-round-authorizes-concurrent-turns.md)

## Context

This crate has, until now, offered exactly one shape: a room of agents
deliberating **one** question on a shared floor. Every mechanism in it —
salience, quorum, cross-inhibition, the attention market — serves that shape.
Two experiments then asked whether the shape is worth its price, and the
answers disagreed with each other.

[The long horizon](../experiments/2026-09-09-the-long-horizon.md) grew the task
through *time*: a chain of sub-decisions on one accumulating window. The room
lost, decisively and at every setting. A single agent compacting by a
superseding account scored **13.1%** end-to-end against the room's **5.1%** at
sixteen stages and an eight-row window, and spent a seventh of the depth doing
it. Length is not what a room is for, and the experiment says so in its own
record rather than being quietly filed.

[Variety and roles](../experiments/2026-09-09-variety-and-roles.md) grew the
task *sideways*: several independent sub-decisions belonging to one task, each
wanting a different kind of attention. Here the ordering flipped, with the
crossover at **two**:

```text
all-facets %       F=1      F=2      F=4      F=8
one agent         78.7     56.9     28.1      7.8
one seat each     78.7     61.1     35.9     13.0
on-floor room     68.2     46.8     21.1      4.1
```

Three controls bound that: at an unbounded window every arm is identical, so
the advantage is entirely the context window; without roles the effect
disappears (33.9 against 34.0), so it is bought by dividing along a line where
*competence* differs rather than by dividing at all; and a seat that splits
without pooling its peers' readings of its own facet scores **1.7**, worse than
not splitting.

The shape that wins was assembled **in the benchmark**, out of parts the
library does not offer. A host reading the write-up would have had to build it
itself, from a paper, without the controls.

## Decision

**`tinyhivemind-hive` ships the division of labour as a first-class pure fold,
and its default is on.**

1. A new module, `division`, folds a task's facets plus a desk snapshot into a
   `Division`: who owns each facet, which assignments ride the same round, and
   whether the owner was chosen by competence or by rotation.
2. `Division::scoped` carries the other half — an owner reads its own facet's
   rows and none of the others'. That is the half the experiment attributes the
   win to, so it belongs beside the assignment rather than in a host that might
   forget it.
3. `DivisionPolicy::DEFAULT` **divides, and follows the directory.** Every
   other opt-in mechanism in this crate ships off, because each was measured
   and lost or broke even. This one was measured and won, and a default that
   made a caller opt into the winning arm would be the wrong way round.
4. A task of one facet divides into one assignment in one round — a single
   agent answering alone. This is **not** a special case in the code. It is the
   measured rule falling out of the general one, and making it a branch would
   invite the branch to drift from the rule.

## Consequences

**The floor is not removed, and is not deprecated.** `step` is what decides a
*facet* once its owner cannot decide it alone, and it remains the arm the
division was measured against. An arm that loses is kept here, with its matched
control — that is the `hive+mute`/`hive+hush`/`hive+quiet` discipline, and
retiring the loser would make the comparison unrepeatable.

**Purity is unchanged.** `divide` is a fold over arguments the caller holds. It
opens nothing, awaits nothing, and stores nothing between calls; `depth()` is a
*number of rounds* and the host does the waiting. `assert-pure.sh` passes
unchanged, and no dependency was added.

**Two bounds, for ADR 0014's reason.** A round runs at most
`DivisionPolicy::round_width` assignments and never names one seat twice. What
the removed one-turn rule was guarding was unbounded fan-out; the bound is the
invariant that replaced it, and a division inherits it rather than inventing a
second policy for the same danger.

**The benchmark now measures the shipped mechanism.** `hive+fold` was assembled
inside `examples/bench` before this module existed. It now calls `divide`, and
reproduces every published cell bit-for-bit — 78.7 / 61.1 / 35.9 / 13.0, depth
1 / 1 / 1 / 2. That identity is the acceptance criterion, asserted rather than
assumed, exactly as `round_width: 1` was for ADR 0014.

**A new error variant.** `Error::NoSeats { desk_id }` — a task with facets and
a desk with no active member cannot be answered, and returning an empty
division would look like success.

## What would falsify this

The soloist this was measured against aggregates by an unweighted mean, so it
averages one sharp reading with four wide ones. A soloist that knew whose
reading to trust would narrow the gap, and if it closed it this default should
go back off. The room is given no such knowledge either, so the comparison is
between two equally naive aggregators — which is a fair fight and not a
flattering one.

The result does **not** rest on how much a summary is worth: swept from 0.35
down to 0.0, which is compaction by pure eviction, the division leads at every
value.
