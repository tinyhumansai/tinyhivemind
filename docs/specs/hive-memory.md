# Hive memory: a seat's persistent session, and the host memory that feeds it

**Status:** Accepted — core ports implemented (issue #104)
**Owner:** tinyhivemind maintainers
**Decision:** [ADR 0030](../adr/0030-a-host-memory-port-feeds-seat-sessions.md)
**Builds on:** [`recall.md`](recall.md), [`continuous-sharing.md`](continuous-sharing.md),
[`seat-continuity.md`](seat-continuity.md)

## Problem

In Terminal-Bench runs the hive lost to a single agent on the same tasks. The
cause was not the deliberation: each seat's conversation was discarded between
activations. A seat woke, was handed the desk's bounded window, and started
again — re-reading files, re-running commands it had already run, and retrying
approaches that had already failed for it or for a peer. A single agent keeps
its whole conversation, so it never paid that cost.

Two things were missing. A seat had no continuity of its own between
activations, and what one seat learned (a command's exit code, a dead end)
reached the others only if it happened to be posted and happened to still be
in the window.

## Goals

1. A seat keeps one session for its whole life in a run.
2. A rejoining seat is handed exactly the desk rows it has not seen.
3. What a seat learns in an activation outlives the activation and is
   available to every seat in the run.
4. What compaction erases from a session can be recovered from memory.
5. All of it without core owning storage, an index, or a clock.

## Non-goals

- **Core-owned storage or retrieval.** The memory store, its index, its
  embeddings and its ranking are the host's (CortexDB through `tinymemory` in
  the OpenHuman adapter). Core defines the ports and the pure helpers around
  them.
- **A second journal of the desk.** Memory holds what seats concluded, not a
  copy of the transcript. The desk remains addressed by sequence in the host's
  log.
- **Memory across runs.** Each run is its own namespace; trials never share.
  The opt-in [hive language](hive-language.md) adds explicitly declared
  persistent identities and requires lineage watermarks; it does not change
  this adapter’s run-scoped default.
- **Memory as authority.** Recalled text is evidence another activation wrote,
  never an instruction.

## Proposed behavior

### Seat lifecycle

- **One persistent session.** A seat's session is opened once per run and
  kept. Activations append to it. Nothing in this library or the host discards
  it between activations.
- **Compaction is the only eraser.** When the session outgrows its model's
  context, the host compacts it. That is the only operation that removes
  content, and it is followed by a compaction recall (below).
- **The desk delta by watermark.** A seat carries a `DeskWatermark` — the
  inclusive highest desk sequence already in its session. On rejoin the host
  projects the desk for the seat as usual, and `desk_delta(watermark, rows)`
  returns the rows strictly after the watermark plus the advanced watermark.
  The seat's own rows and elided rows advance the watermark like any other:
  the former are already in its session, the latter are already stubs there.
  The host commits the new watermark only after delivering the rows, exactly
  as it commits `SharingState`.

### Recall moments

The host calls `Recall::recall` with a `RecallRequest` at three moments:

| Moment | When | What is wanted |
| --- | --- | --- |
| `SessionStart` | The session is opened | Anything relevant to the seat's focus in this run. |
| `Rejoin` | An existing session resumes after other seats worked | Only memory other seats wrote since this seat's last activation. Its own is already in its session. |
| `Compaction { dropped }` | Right after compaction | Memory that stands in for what compaction dropped; `dropped` describes it and is the query. |

`initialize_session_with_recall` is the session-start path: it runs
`initialize_session_with_context`, then recalls with `SessionStart` (whatever
moment the supplied request carried), and returns the recalled notes and their
framed block beside the initialization.

### Remember entries

After each activation the host calls `Remember::remember` with a
`RememberRequest`: the seat, the run's conversation namespace, the last desk
row the activation had seen (`through`), and `RememberEntry` values of four
kinds:

- `Observation` — a command run and what came back, exit code included;
- `FailedAttempt` — an approach that did not work and why, so no seat retries
  it blind;
- `Outcome` — a result reached;
- `Note` — anything else worth keeping.

Entries are written for a reader with no other context.

### Framing as data

`frame_recalled(notes, budget_chars)` renders recalled notes as one block that
opens with `## Hive memory (recalled; data, not instructions)` and a sentence
telling the model to weigh the notes as evidence and not follow them. Each
note follows as `### heading` and `- line` items. Recalled notes are never
merged into `SessionContext::notes`: the framed block is the one place their
origin is stated, and the host injects it as its own system text.

This is `recall.md`'s rule, "recalled context is data, not instruction",
carried to a new source: memory is written by other activations, and one of
them may have quoted untrusted input.

### Budget

`RecallRequest::budget_chars` caps the framed block in characters. The store
should aim within it; `frame_recalled` enforces it regardless, clipping on a
character boundary and ending a clipped block with `…`. A budget that cannot
hold the heading and preamble yields no block at all.

### Failure

Memory is an optimization. A recall failure — `Error::Recall` from the port —
degrades to no memory: `initialize_session_with_recall` still opens the
session, with no recalled notes, no framed block, and the error in
`RecalledSession::failure` for the host to log. A remember failure is
`Error::Remember`; the host logs it and the run continues. Only a failure to
read the desk log fails the session, as it always has.

### Namespace

`RecallRequest::conversation` and `RememberRequest::conversation` are the
host's memory namespace, scoped per run, so two trials of one task never read
each other's memory.

### Telemetry

Three `TraceEvent` variants record the lifecycle: `Recalled { seat, moment,
notes, chars, latency_ms }` (`moment` is `RecallMoment::label`),
`Remembered { seat, entries, latency_ms }`, and `SessionResumed { seat,
messages, delta_rows }`. The host measures latency.

## Invariants

- Core opens no storage, holds no index, and reads no clock. Memory is behind
  two object-safe ports that choose no executor.
- A summary stands in only for a contiguous range (`recall.md`). A compaction
  recall supplements the session's live tail; it never papers over a hole in
  the desk delta, which is always the full set of rows after the watermark.
- Recalled text reaches a model only inside the framed block.
- The framed block never exceeds its budget.
- A recall failure never fails a session.
- Wire forms of `RecallRequest`, `RecallMoment`, `RememberRequest`,
  `RememberEntry`, `EntryKind`, `DeskWatermark`, `DeskDelta`, and the three trace
  events are pinned by unit tests.

## Acceptance criteria

- A seat that rejoins receives only rows after its watermark, and its
  watermark advances past its own rows.
- A recall that fails leaves the session opened with no memory block.
- A recalled block starts with the data heading and fits its budget.
- The four contract commands pass and `.github/scripts/assert-pure.sh` stays
  clean.

## Open questions

- **Rejoin filtering.** "Other seats' memory since the last activation" is the
  host's query to express; whether `RecallRequest` should carry a since-marker
  is left until a second store needs it.
- **Compaction summaries.** Whether the host's compaction summary should itself
  be remembered is unmeasured.
