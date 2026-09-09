# What a concurrent round is worth, and which half of it is free

**Date** 2026-09-09
**Status** Recorded
**Code** `cargo run --release -p tinyhivemind-hive --example bench` on
`concurrent-rounds`, at `--seed 1 --episodes 5000`; the size table from
`--scale-sweep --hidden-profile --episodes 1000 --sizes 3,5,8,16,32,64`.
**Spec** [`../specs/concurrent-rounds.md`](../specs/concurrent-rounds.md)
**Decision** [ADR 0014](../adr/0014-a-round-authorizes-concurrent-turns.md)
**Sample** 5000 and 1000 seeded rooms, every arm deciding the same rooms.
Simulated participants, so the numbers reproduce from the seed.

## The question

Two questions, and the second only became askable once the first was answered.

**What does an episode cost?** Every table in this harness priced work in
`turns/ep`, which is only a cost if turns are serial. A seat is an async
session, so `vote`'s fifteen turns are *one round* of fifteen parallel calls
while `hive+`'s 6.75 are 6.75 sequential ones. The recorded
`hive+ − vote: +3.6 [+2.1, +5.0]` buys 3.6 points for roughly seven times the
wall clock, and no column could say so.

**And is concurrency worth having?** ADR 0002 said no, and said what would
change its mind. [`2026-09-09-topology-at-scale.md`](2026-09-09-topology-at-scale.md)
supplied it: every floor-bound arm reads 0.0% by thirty-two members, because
turns per member is `turn_budget / n`.

## The cost model

`rounds/ep` is now reported beside `turns/ep`: depth beside width, what a host
waits for beside what the budget bounds. The first thing it prints is the
comparison the benchmark could not previously express.

```text
arm       turns/ep rounds/ep   decided %   correct %
ladder        1.00      1.00       100.0        57.6
vote         15.00      1.00       100.0        78.5
hive+         6.75      6.75        99.4        82.1
```

`vote` is depth **one**. Every one of its answers is formed from a member's own
private evaluation alone, having seen nothing, so no call in it waits on any
other. The matched-budget control was never spending more wall clock than the
deliberation; it was spending less, by a factor of about seven, and being
charged for the opposite.

## The result: width is two mechanisms, not one

```text
arm       turns/ep rounds/ep   decided %   correct %
hive+         6.75      6.75        99.4        82.1
hive+wide     7.64      3.38        89.5        77.1
hive+blind    6.75      3.75        99.4        82.1
```

- `hive+wide` widens **every** round to four. Depth halves. Accuracy falls five
  points and the decision rate falls ten.
- `hive+blind` widens **only while the room is blind**. Depth falls 44%, and
  every other column is *identical to `hive+`* — same turns, same decision
  rate, same accuracy, arm for arm.

The reason is one sentence: **a blind member cannot read a peer's row whether
or not it ran concurrently with that peer**, so widening a blind round changes
no projection any participant sees. A revealed member's turn depends on exactly
the row a concurrent peer is writing, so widening there spends information for
depth.

That is why `EpisodePolicy` carries two bounds rather than one. `round_width`
defaults to four and `revealed_width` to one: all of the free concurrency, none
of the paid kind.

### It only became free once the blind round stopped overshooting

The first cut of `hive+blind` scored 81.7% at 8.42 turns against `hive+`'s
82.1% at 6.75. The extra turns were the mechanism paying for itself: with five
members and a width of four, the second blind round authorized four more turns
when only one member was still unheard. Capping a blind round at the number of
members *not yet heard* removed the overshoot, and with it the whole of the
loss. A wide blind round is now exactly the same turns as a narrow one, in
fewer waits.

## Across the room sizes

Hidden profile, 1000 rooms per size.

```text
correct %           3        5        8       16       32       64
hive+            31.3     16.0      8.7      1.0      0.0      0.0
hive+blind       31.2     16.0      8.7      1.0      0.0      0.0
hive+wide        29.3     14.9      5.2      0.7      0.0      0.0

rounds/ep           3        5        8       16       32       64
hive+             4.5      6.4      9.6     17.2     33.1     65.1
hive+blind        2.5      3.4      3.6      6.9     14.7     31.0
hive+wide         2.4      3.3      3.3      6.8     14.7     31.0
```

`hive+blind` tracks `hive+`'s accuracy to a tenth of a point at every size, at
**less than half the depth** — 31.0 rounds against 65.1 at sixty-four members.
`hive+wide` loses accuracy at every size where there is any left to lose.

## What this does **not** show

**Concurrency does not rescue the scale collapse.** `hive+blind` is 0.0% at
thirty-two members, exactly as `hive+` is. Depth was never why every
floor-bound channel dies on a hidden profile; the lone fact-holder being one
voice among more and more of them is, and a round changes when members speak
rather than what any of them can read. The finding in
[`2026-09-09-topology-at-scale.md`](2026-09-09-topology-at-scale.md) stands
untouched, and `hive+fact°` is still the only arm above zero.

**The prediction in ADR 0014 was half wrong, and is corrected there.** It
argued that because a concurrent round is a blind round, and because this
repository measures the blind opening at 24 points, wide rounds should raise
accuracy as well as cut depth. They do not. Independence is worth a great deal
*once*, at the opening, and nothing afterwards — a member that cannot read its
peers cannot update on them either, which is what `hive+wide` is paying for.

**One task family, simulated members.** The participants are arithmetic. What
is measured is whether a policy moves information and how many waits it takes,
not whether a model would write a useful row.

**Nothing here was run live.** No model, no desk, no wall clock actually
measured — `rounds/ep` is a count of rounds, not seconds. What a host saves is
that count times whatever a turn costs it, minus whatever its own fan-out
costs, and this harness measures neither.
