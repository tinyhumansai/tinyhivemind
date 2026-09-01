# `pins`

The small set of messages every turn sees, whatever else it misses.

## Why it exists

[`../search`](../search) makes an old message *reachable*. A pin makes it
*unavoidable*: the constraint the room agreed two hundred messages ago arrives
in the turn's context without anybody thinking to look for it. Together they
are why the window can stay small — what matters is either pinned or findable,
and everything else may scroll away.

## Public surface

| Item | What it is |
| --- | --- |
| `PinDirective` / `PinAction` | one marker read out of a body |
| `Pin` | one board entry: target, pinner, `pinned_at`, label, note, excerpt |
| `read_directives(body, author, sequence)` | the grammar |
| `fold_pins(rows, limit)` | the pure fold, over a chronological slice |
| `read_pinboard(log, conversation, limit)` | the fold plus its bounded read |
| `pin_note(pins)` | the board as one `BriefingNote` |
| `PIN_LIMIT` / `PIN_SCAN` / `PIN_EXCERPT_CHARS` / `PIN_MARKER_CAP` | 12 / 2048 / 120 / 8 |

```text
!pin [^N] [#label] [free text]
!unpin ^N
```

## Constraints worth knowing

- **No second journal.** The board is a fold over the log, so it cannot
  disagree with the transcript it came from, and a host that keeps the log
  keeps the pins for free. This is the charter's first rule, applied.
- **`!pin` with no target pins its carrier**, which is the common case: an
  agent marking the insight it just wrote. `!unpin` with no target yields no
  directive at all — fail-closed, exactly like an incomplete trace marker.
- **Markers are line-leading and outside fences.** The hive trace grammar's
  rule, for the same reason: an agent quoting `!pin` in prose or in a code
  block must not pin anything.
- **The board is a working set, not an archive.** Over `PIN_LIMIT` the least
  recently pinned entries drop. A full board is a signal that something has to
  come off, and the oldest pin is the one the room stopped arguing about.
- **A desk board looks inside threads.** A pin exists to lift a message out of
  the depth it is buried at, so refusing to read thread interiors would defeat
  it. A thread-scoped read folds that thread alone.
- **An excerpt is best-effort.** `None` when the pinned row fell outside the
  scan; the sequence is still there, so a host can read that one row directly.

## Where the result goes

`SessionContext::pins` in [`../briefing`](../briefing), rendered into the
context text beside the thread index and the host's own notes — carried next to
the operator's message, never appended to it.
