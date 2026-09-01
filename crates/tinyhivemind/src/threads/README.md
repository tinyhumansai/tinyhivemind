# `threads`

A bounded, recency-ordered index of the live threads in one desk.

## Why it exists

An agent that has just cold-started, or that has been away, is asked "what are
we doing?" and can only answer from whatever fit its window. The window is a
*recency* bound over rows, so a thread that has been quiet for a page is
invisible even though it is the one that matters.

The index answers the question directly: the roots in this desk, what each one
opened with, how much reply traffic it has drawn, and when it was last touched.

## Public surface

| Item | What it is |
| --- | --- |
| `ThreadLine` | one indexed thread: `root`, `opening`, `replies`, `latest`, `landed` |
| `fold_thread_index(rows, limit)` | the pure fold, over a chronological slice of one desk |
| `read_thread_index(log, conversation, limit)` | the fold plus its bounded read |
| `THREAD_INDEX_LIMIT` | default rows described to a viewer (5) |
| `THREAD_OPENING_CHARS` | characters of a root kept as its opening (60) |
| `THREAD_INDEX_SCAN` | maximum raw rows inspected for one index (256) |

The fold is separate from the read on purpose: it is the part worth testing, and
it needs no fixture. This is the charter's rule — the decision in the fold, the
waiting behind the port — applied inside one module rather than across the two
crates, because the fold's input is `LogMessage`, the port's own row type.

## Constraints worth knowing

- **`landed` is the host's.** Where a thread's work ended up is board state this
  crate does not hold and must not learn. The fold always returns `None`; a host
  fills the field afterwards. Nothing else on the row needs the host.
- **The scan bound is deliberately small** — 256 rows against `SCAN_LIMIT`'s
  2048. The index is a recency view; paying a full scan to surface a thread
  nobody has touched in two thousand messages is the wrong trade. The visible
  consequence: a thread whose *root* fell outside the bound is absent even when
  its replies are recent, because a reply alone cannot supply an opening.
- **A blank root is not a thread.** The row exists to say what a thread is
  about, and a blank one says nothing, so neither it nor its replies are
  indexed. This differs from the channel-level projection, where a blank root
  still anchors its first reply — there the root's job is to *be* an anchor, not
  to be read.
- **No index inside a thread.** `read_thread_index` returns empty, without
  reading, for a conversation that already has a `thread_root`: a viewer inside
  a thread is not choosing between them.
- **Ordering is total.** Threads sort by `latest` descending, then `root`
  descending. Sequences are unique per thread, so the tie-break is unreachable
  through the fold and exists to keep the ordering total by construction.

## Where the result goes

`SessionContext` in [`../briefing`](../briefing), alongside host-supplied
`BriefingNote`s — carried *beside* the operator's message, never appended to it.
See [`docs/specs/thread-scoped-conversations.md`](../../../../docs/specs/thread-scoped-conversations.md).
