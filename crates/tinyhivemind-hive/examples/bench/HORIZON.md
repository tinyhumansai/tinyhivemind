# A task with a horizon

Split out of [`README.md`](README.md), which is capped at 500 lines. The full
argument and every caveat are in
[`the experiment`](../../../../docs/experiments/2026-09-09-the-long-horizon.md);
this is the file-level reference for what `--stages` builds.

## The chain

`--stages N` runs N sub-decisions in sequence, decided by the same members.
Three things make it a horizon rather than a repeat:

- **Stage `k`'s options are named distinctly**, so a reading taken at one stage
  can never be scored against another's question. It can only take up a row.
- **Context accumulates.** What a member was told at stage `k` is still in its
  window at stage `k+1`, relevant or not.
- **A wrong stage poisons the next**, by `POISON_LIFT` on the following stage's
  decoy — the room is reasoning from a false premise. Bounded below the 60-point
  true-to-decoy gap, so recovery stays possible and is deliberately hard.

Every member is charged one row per option for the brief it was handed, so a
soloist given every peer's brief carries `agents` times what one member does.

## The arms

| arm | what it is |
| --- | --- |
| `solo` | one agent handed every member's readings and facts, deciding each stage alone from its own argmax. One turn, one round. Compacts by **eviction** |
| `solo+fold` | the same agent compacting by a **superseding account**: a row past capacity gives up its position and still counts at `--fidelity` |
| `hive+` | the room, deliberating each stage at the tuned policy |
| `hive+pooled` | the ceiling |

## What it found

```text
end-to-end %, 16 stages    solo   solo+fold   hive+
window unbounded           26.1        26.1     5.1
window 32                  24.1        24.1     5.1
window 16                   4.8        17.5     5.1
window 8                    0.5        13.1     5.1
```

The crossover against `solo` is real and arrives at a window of about sixteen
rows: the soloist carries 320 rows at sixteen stages and each member of the room
carries 64. Against `solo+fold` there is no crossover at any window measured.

`hive+` is **identical in every cell** because a window never binds for a member
of the room: its pooling happens through the floor, and this harness does not
charge a transcript read to anybody's window. That favours the room, and the
room loses anyway.
