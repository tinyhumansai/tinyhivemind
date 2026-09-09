# Thoughts and channels: what an agent holds, and what it says

**Status:** Draft — the library half is implemented (`tinyhivemind::digest`), the
host half is implemented in the `desk` example, and the live comparison is
pending
**Owner:** tinyhivemind maintainers
**Reading:** [`../research/multi-context.md`](../research/multi-context.md),
[`seat-continuity.md`](seat-continuity.md)
**Evidence:** [`../experiments/2026-09-09-desk-lessons.md`](../experiments/2026-09-09-desk-lessons.md)

## Problem

Two defects, and they turn out to be one.

**A window is not a memory.** `project_session` hands a turn the newest rows it
may read. That is right for answering the room and wrong for knowing it: by run
26 a ten-row window held five copies of the task brief, and a seat joining on
turn forty would have been handed thirty rows and no account of the four
hundred before them. Every reaction to that is worse than the disease — a
bigger window, a shared scratch file that is everyone's and therefore nobody's
(`NOTES.md` reached 37k characters), or a seat that re-reads twelve files
before it thinks.

**Speech is a string.** A seat said things by writing `<<<POST … POST>>>` into
its own output. In run 26 a seat closed with the symmetric spelling instead and
a verified sublinear recursion reached the transcript as the three characters
`>>>`. The room read it as silence and the chair nudged the seat that had just
spoken. The extractor now accepts both fences, which fixes that spelling and
not the class: a model-authored delimiter is an interface with a fallible
producer, and it has no schema, no validation, and no way to tell the producer
it got it wrong.

They are one defect because they share a cause. The room was a *string the
agent emitted*, so everything about the room — who it reached, what it
contained, how much of it a reader saw — was carried in prose and settled by
parsing.

## Goals

1. A seat's output text is its own thinking. What reaches the room is an action
   it takes, with a schema, that can be refused back to it.
2. A channel carries an account of itself, bounded and superseding, so what a
   turn spends its window on is the live conversation rather than its own
   scrollback.
3. A seat that has never spoken can be given the state of a long channel in one
   page.
4. Agent-to-agent address is a field, not a convention in prose.

## Non-goals

- **Relaxing one message, one turn.** A seat still speaks once per turn.
- **A model in the chair.** The controller stays deterministic. The one model
  call this adds writes an account nobody is quoted in.
- **A second journal.** The account supersedes itself and is not in the log.
- **Concurrency.** Nothing here lets two seats run at once.

## Proposed behavior

### 1. The room is a tool

A seat reaches the room through three tools, served over MCP by the host:

| tool | what it does |
| --- | --- |
| `desk_post(message)` | say one thing to the whole desk |
| `desk_dm(to[], message)` | say it to named seats instead |
| `desk_read(limit)` | read further back than the window it was handed |

Text a seat produces outside a tool call is its own thinking and reaches
nobody. Consequences, in order of how much they were worth:

- **A malformed utterance is refused to its author**, inside its own turn,
  with a reason, while it can still fix it. A fence can only be
  mis-parsed after the fact.
- **The address is a field.** `desk_dm` names its recipients; the host does not
  read the intent out of the prose.
- **Silence is legible.** A turn that called nothing said nothing, and the
  host knows the difference between that and a turn that spoke and was
  garbled.
- **`desk_read` makes the window a floor rather than a ceiling.** A seat that
  needs more of the room can fetch it, so the window can be *smaller*.

The server writes nothing to the transcript. It appends to a per-turn outbox
the host drains after the process exits, so sequence assignment, audience
resolution through `aside`, mention resolution and dispatch all stay where they
were. **A tool call is a request to speak; the host still decides what that
means.** In particular `desk_dm` is resolved through the existing aside policy
and may be refused, in which case the row stays desk-visible — failing toward
the room rather than toward silence.

A seat may call `desk_post` twice. The last call stands.

The fence remains as a documented fallback for an agent CLI that cannot reach
the tools, and the tool-less wrap-up completion still uses it, because a plain
completion has no tool to call.

### 2. A channel carries an account of itself

Each channel carries one **account**: a bounded text standing for every row
older than the live tail, rewritten as the room moves, always covering strictly
more than before. A turn is composed of the account plus the rows the account
does not cover.

- **Bounded and superseding.** `DigestPolicy` states `keep_live`, `fold_after`,
  `input_limit` and `budget_chars`. A fold advances by at most `input_limit`
  rows, so a first fold over a long history is a run of bounded calls rather
  than one enormous one, and no row is stepped over.
- **The fold is a port.** `Digester` takes a request and returns text: no host
  handle, no callback, no tools. A missing or failing digester is
  `DigestOutcome::Unavailable` and not an error — compaction is an optimization
  over a projection that is already correct without it.
- **An answer can be refused.** Empty, over budget, or covering no more than
  what it replaces are `DigestRejection` values. The held account stands.
- **It is host state, not a row.** An append-only log that never condenses
  cannot hold a superseding record without growing without bound or being asked
  to forget, and the charter forbids the second.

### 3. An account never contains a private row

`collect_digest_input` folds only rows admitted to every member — `Audience::Desk`
and nothing else. This is what makes one account serve every member of a
channel, including one that has never spoken, and it means no fold can launder
a private row into a shared summary.

An aside's *contents* therefore leave the readable record when they scroll out
of the window. That is the existing `must_surface` bargain: an aside owes the
room a surfaced conclusion, and the conclusion is a desk row that the account
folds like any other.

### 4. The rows survive

Folding changes what a turn is *shown*, never what the log holds. Every folded
row keeps its sequence, `search_messages` still finds it, and `desk_read`
reaches it. The account is derived and lossy, and the prompt says so, which is
what makes it safe to be lossy.

## Invariants and constraints

- One message, one turn. `desk_post` produces one row; a second call replaces
  the first rather than adding a turn.
- The transcript is append-only and never condensed. Nothing here edits or
  removes a row.
- The library holds no notebook, no outbox, and no account — all three are host
  state. The library holds the *rules* about them.
- The account is never quoted as anybody's words. It is attributed to the desk,
  it names seats as `@id` and cites rows as `^N`, and no seat is made to have
  said something it did not say.
- `tinyhivemind-core` gains nothing. Every type here needs `Sequence` and
  `SessionMessage`, which live in the runtime crate, and the fold needs a port.

## Acceptance criteria

1. A seat's `desk_post` call becomes exactly one transcript row, and text
   outside a tool call becomes none. **Met** — offline run, 16 turns.
2. A `desk_dm` call produces a row whose audience is an aside resolved by the
   library, or a desk row with a printed refusal reason. **Met** — one
   `{"kind":"aside","members":["lead"]}` row.
3. Once a channel outgrows `keep_live + fold_after`, an account appears in every
   subsequent prompt, and rows it covers are not also shown in full. **Met** —
   generation 1 covering 20 rows through `[21]`, 1290 characters.
4. No account contains the content of a non-desk row. **Met** — the aside at
   `^17` appears in no account.
5. A restart adds no rows before the first agent turn; a seat's notebook
   reaches its next turn. **Met** — carried from
   [`seat-continuity.md`](seat-continuity.md).
6. `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features --
   -D warnings`, `cargo build --all-targets --all-features`,
   `cargo test --all-features`, and `.github/scripts/assert-pure.sh` all pass.
   **Met** — 811 tests, 0 failures.
7. On the same task and desk file as runs 26–27, per-turn `read` calls and
   messages-per-minute are recorded and compared. **Not met** — run 28 solved
   PE 1006 where runs 26 and 27 did not, but its read counts are not comparable
   to theirs: both baselines resumed a 29-file workspace (run 27's task file
   opens "Read NOTES.md, then start here") while run 28 began with an empty
   directory, which explains the gap at least as well as any mechanism here.
   Nor does the run test this spec's mechanisms: no fold fired (at `--window 12`
   a fold needs more than 32 rows and the desk closed at 23), the solving turns
   carried no notebook, and turns 1–2 posted through the landing phase without
   calling `desk_post` at all. The tool room's demonstrated gain is narrower —
   10 clean posts out of 10 on turns 3–12, against run 26's malformed fence.
   A comparable run means run 28's configuration started from run 27's
   workspace. See
   [`2026-09-08-pe1006-tool-room.md`](../experiments/2026-09-08-pe1006-tool-room.md).

## What this does not yet do

- **Per-seat accounts.** One account per channel, folded from desk-visible rows
  only. A seat with a long private thread of asides gets no account of it.
- **Deltas against an account.** `prepare_delta` and the account are two ways to
  avoid resending history and they do not yet compose: a seat being caught up
  incrementally is shown no account, on the grounds that it already holds the
  rows.
- **Reading a fold back.** `desk_read` returns rows, not the accounts they were
  folded into, and there is no way to ask "what did the account say at
  generation 3".
- **A pinned brief.** The brief is appended once rather than pinned, which is
  the cheaper half of that fix.

## Open questions

- **What does the account cost when it is wrong?** A lossy summary that drops
  the one fact the next turn needed is a defect with no error message. The
  mitigations are that the rows survive and that `desk_read` reaches them, but
  nothing measures how often a seat actually goes back for them.
- **Should `fold_after` be measured in tokens rather than rows?** Twenty rows of
  one-line acknowledgements and twenty rows of derivations are the same trigger
  and very different costs.
- **Feedthrough rows spend the window** — carried unresolved from
  [`seat-continuity.md`](seat-continuity.md), and the account changes its shape:
  a folded feedthrough row becomes one clause naming a file, which is most of
  what it was worth.
- **Does a smaller live window now beat a larger one?** The account exists to
  make that trade available and the run has not been done.
