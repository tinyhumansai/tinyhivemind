# What a channel is worth as the room grows

**Date** 2026-09-09
**Status** Recorded
**Code** `cargo run --release -p tinyhivemind-hive --example bench -- --scale-sweep`
on `scale-topology`, with `--hidden-profile --episodes 1000 --sizes 3,5,8,16,32,64`;
the federation figures from `--swarm --desks N --per-desk 4 --episodes 200`.
**Sample** 1000 seeded rooms per size, every arm deciding the same rooms.
Simulated participants, so the numbers are reproducible from the seed rather
than sampled from a model.

## The question

Every recorded number in this repository is for a room of five. The size table
on the wiki already shows something moving — `hive+`'s margin over the
matched-budget vote decays from `+3.2` at three members to `+1.2` at eight —
and it stops at eight because a fixed eight-name table in the harness stopped
it, not because anything about the library does.

So: **at what room size does the way members reach each other start to matter,
and which way.**

## What had to change first

`MEMBER_ROLES` was a fixed array of eight and `--agents` clamped to its length;
`DESK_NAMES` was four and `--desks` clamped to that. Both are now generated past
the named entries, and both keep the original names for the first entries so
every recorded number reproduces exactly rather than approximately — verified:
`ladder 57.6 · vote 78.5 · hive 73.3 · hive+ 82.1` at 5000 rooms, unchanged, and
the swarm arm still lands on `swarm 77.0 · pooled 74.5` against a record of
`77.5 · 74.5`.

## Uniform noise is the wrong task at scale

Run the ladder past eight on the default task and the answer is that nothing
matters:

```text
agents      8       16       32       64
vote     83.3     96.7     99.3    100.0
hive+    83.3     95.0     99.3    100.0
```

A plurality of independent noisy readings converges on the truth by itself as
the room grows, so by sixteen members there is nothing left for a protocol to
add and both arms saturate. A benchmark run there measures the law of large
numbers.

The regime worth sweeping is the one where pooling does not trivially win.

## The result

Hidden profile — a decoy above every member's own argmax except one, who alone
holds the fact that rules it out. 1000 rooms per size.

```text
arm                 3        5        8       16       32       64
ladder           44.6     33.8     28.4     27.0     22.5     19.4
vote             31.2     15.9      7.4      1.1      0.0      0.0
hive+            31.3     16.0      8.7      1.0      0.0      0.0
hive+fact        24.1     15.8      9.8      0.9      0.0      0.0
hive+rounds      31.3     16.1      9.1      1.1      0.0      0.0
hive+fact°       56.7     44.7     32.9     17.6      6.4      1.1
hive+pooled      95.7     91.9     87.0     85.2     87.4     89.8
```

Three things fall out, and the third is the one worth building on.

**Every floor-bound channel dies.** `vote`, `hive+` and on-floor pairwise
(`hive+fact`) all reach **0.0%** by thirty-two members. Growing the room makes a
hidden profile monotonically worse for any majority mechanism, because the lone
fact-holder is one voice among more and more of them.

**The information never leaves the room.** `hive+pooled` — every reading and
every fact in every member's hands, free — is flat at 85–96% across the whole
ladder. Nothing is lost as the room grows; what fails is every mechanism for
moving it.

**Only an off-floor channel survives.** `hive+fact°`, the aimed check carrying
the fact rather than a number, held *before the episode opens so it spends no
turn*, is the single arm above zero at every size. At five members it is
`44.7` against `hive+`'s `16.0`; at thirty-two it is `6.4` against `0.0`. In
absolute points its lead decays — but it is the only thing that is not nothing.

This is the same law [ADR 0011](../adr/0011-an-aside-rides-alongside-a-turn.md)
and [ADR 0012](../adr/0012-an-exchange-round-spends-model-calls-not-turns.md)
established at five members, and the room size is what makes it loud: an
exchange that costs a turn cannot pay for itself, and the bigger the room the
more turns the floor is already spending.

### And an off-floor channel is not free either

```text
contacts/ep      3        5        8       16       32       64
hive+rounds   13.5     32.0     76.5    275.3   1059.1   4173.7
```

`hive+rounds` — continuous off-floor exchange, everybody contacting somebody on
every turn — costs model calls that grow roughly with the square of the room and
buys **nothing**: `31.3 → 0.0`, identical to `hive+` at every size. What works
is the *aimed, bounded, fact-carrying* single check, not volume. A host reading
this should size `exchange_cap` against who needs reaching, never against how
many peers exist.

## The federation collapses too, and for a nameable reason

```text
desks          3        6       12       24
swarm       78.0     66.5      0.0      0.0
pooled      74.5     87.0     97.5     99.5
```

At twelve desks the swarm arm decides **nothing at all**. The diagnostic says
why, and it is not that referral cannot reach:

```text
arm        correct  decided     turns  crossings  stranded
swarm         0.0%        0     370.0      246.0      20.0
swarm  desk endings: converged 0 · deadlocked 0 · exhausted 2400 · idle 0
```

Every desk exhausted its turn budget. A referral costs the *answering* desk a
floor turn, and a desk in a federation of twelve is asked by eleven peers while
its own budget stays what it was for a federation of three. The incoming load
scales with the federation and the budget does not, so the federation spends
itself answering and never decides. Twenty answers are stranded on top of that.

That is the on-floor exchange law again, one level up: inside a room it costs
the asker a turn, and between desks it costs the answerer one. The fix implied
is the same one — a referral should be answerable off the floor — and it is not
implemented.

## What this does not show

- **One task.** A hidden profile is the adversarial case on purpose. It says
  nothing about a room whose members are merely noisy, where the uniform table
  above shows every mechanism saturating.
- **Simulated members.** The participants are arithmetic. What is measured is
  whether a policy moves information, not whether a model would write a useful
  row.
- **`hive+fact°` is given oracle targeting.** It picks its peer from private
  room state rather than from the transcript, so it is an upper bound on what an
  aimed off-floor check is worth, not an implementable arm. The gap between it
  and `hive+fact` is what aiming plus scheduling are jointly worth; the gap
  between it and `hive+pooled` is what is still on the table.
- **Nothing here was run live.** No model, no desk, no PE 1006.
