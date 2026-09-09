# The utterance surface: speaking is a request, not a string

**Status:** Implemented — `tinyhivemind::speech`, with the `desk` example
reduced to transport. The live run that exercises the in-turn refusal is
pending
**Owner:** tinyhivemind maintainers
**Reading:** [`thoughts-and-channels.md`](thoughts-and-channels.md),
[`private-asides.md`](private-asides.md)
**Evidence:** [`../experiments/2026-09-08-pe1006-tool-room.md`](../experiments/2026-09-08-pe1006-tool-room.md),
[`../experiments/2026-09-09-desk-lessons.md`](../experiments/2026-09-09-desk-lessons.md)

## Problem

[`thoughts-and-channels.md`](thoughts-and-channels.md) accepted that a seat
reaches the room through a tool call rather than a model-authored fence, and run
28 was the first run on PE 1006 to produce a verified answer. The mechanism is
right. Where it lives is not.

**Every load-bearing decision is in an example.** The tool schemas and their
descriptions, the outbox wire form, the drain, and the fold from *what a seat
called* to *what the room holds* are `crates/tinyhivemind/examples/desk/mcp.rs`
(431 lines), the house-rules block in `examples/desk/prompt.rs`, and roughly
eighty lines of `examples/desk/run.rs`. A second host cannot depend on any of
it. It will reimplement it, and the places it differs are exactly the ones that
cost runs 26 and 27 — how an utterance is validated, what happens to a malformed
one, and what a private address means.

**"Refused to its author" holds for half the refusals.** A schema error is
returned by the MCP server inside the turn, which is the whole gain over a
fence. A *policy* refusal is not: `aside::address` runs in `run.rs` after the
child process has exited, so a `desk_dm` the aside policy declines becomes a
desk-visible row and the seat is never told it spoke in the open. Failing toward
the room is the right default and is not in question; the author not knowing is.

**The address is a field, and then it is unmade into a string.** `run.rs`
formats the drained `to` list back into `"@a @b"` and re-runs `resolve` over it
to recover targets. The grammar is being used as an internal serialization
format between two pieces of the same host. It is also lossy in a way the field
is not: an id that the mention grammar would not accept, or that
[`masking`](../../crates/tinyhivemind-core/src/masking) would treat as code,
disappears with no error.

**The fence has one specification and two implementations' worth of tests.** It
survives as the documented fallback for a CLI that cannot reach the tools and
for the tool-less wrap-up rung, and its extractor — the one that lost a verified
result in run 26 by reading `<<<POST>>> … <<<POST>>>` — is example code.

## What the mention grammar keeps

Run 28 did not test tools against the grammar, and this spec does not propose
replacing it. The two answer different questions and run 28 used both: a seat
spoke by calling `desk_post`, and *who took the next turn* was still the first
`@id` in the message text, resolved through
[`mention`](../../crates/tinyhivemind-core/src/mention) and dispatched through
[`dispatch`](../../crates/tinyhivemind-core/src/dispatch).

Routing stays text-borne because most of what routes cannot call a tool. An
operator brief, a person's message, the chair's nudge and a folded account are
all strings, and all of them have to be able to hand a turn to a seat. A grammar
read off the body is the only mechanism that serves them uniformly, and it is
the one the whole `tinyhivemind-core` algebra is built on.

Where the two genuinely overlap — the audience of a message — the field wins.
`desk_dm(to)` states its recipients, and this spec makes that statement travel
as targets rather than as prose to be re-parsed.

## Goals

1. The library holds the utterance algebra: what a seat may say, what makes an
   utterance valid, and what exactly one accepted utterance becomes.
2. The tool surface is stated once, as data, so every host serves the same three
   tools with the same descriptions.
3. A policy refusal is computable before the row is appended and is available to
   the host in a form it can hand back to the author.
4. An address stays a field from the tool call to the stored row.
5. The fence has one implementation, one test suite, and both spellings.

## Non-goals

- **MCP in the library.** JSON-RPC, stdio, the outbox file and the CLI config
  block are transport and stay in the host. The library never opens a socket or
  a file; see rule 1 of the charter.
- **Replacing the mention grammar.** See above.
- **Relaxing one message, one turn.** One accepted utterance is one row.
- **`tinyhivemind-core` gaining anything.** Every type here needs `Audience`,
  `Sequence` and a transcript view, which live in the runtime crate.
- **A tool that writes the transcript.** A tool call remains a *request to
  speak*; the host appends and the host decides.

## Proposed behavior

### 1. `tinyhivemind::speech`

A new module in the runtime crate, holding:

- `Utterance` — `Post { message }`, `Dm { to, message }`, `Close { message }`,
  the shape the example already has, with its serde representation pinned.
- `parse_utterance(&Value) -> Result<Utterance, UtteranceRejection>` — the
  validation currently split between the MCP `call` arm and `parse_utterance`
  in the example. `UtteranceRejection` is typed (`EmptyMessage`, `UnknownTool`,
  `NoRecipients`, `UnknownRecipient { id }`, …) and carries the sentence a host
  hands back to the seat.
- `tool_specs() -> &'static [ToolSpec]` — name, description and JSON schema for
  `desk_post`, `desk_dm`, `desk_read` and `desk_close`, as data. A host serving
  MCP renders them; a host on a different tool protocol renders them its way.
  The description text is part of the contract, because it is the only place a
  seat is told that text outside a call reaches nobody.
- `speech::fence` — `extract_post`, accepting both spellings, with the run-26
  regression as a named test.

### 2. `commit_utterance` — one fold, one row

```text
commit_utterance(&CommitRequest) -> Result<CommittedUtterance>
```

`CommitRequest` borrows what the caller already holds: the utterance, the
speaker, the roster, the desk set, and the transcript view `aside::address`
needs. `CommittedUtterance` carries the content, the resolved `Audience`, the
`Vec<Mention>` dispatch will read, `closing: bool`, and
`refusal: Option<AsideRefusal>` naming why a requested aside was declined.

This replaces the block in `run.rs` that resolves mentions, rebuilds the dm list
as text, re-resolves it, and calls `aside::address`. A `Dm`'s recipients become
`MentionTarget`s directly from their ids, checked against the roster.

### 3. A refusal can reach its author

`commit_utterance` is pure and needs no transcript write, so a host may call it
*during* the turn — from inside its tool handler — and return the refusal as the
tool's own result. The seat then knows, while it can still act, that its aside
was declined and its message will be read by the whole desk.

This spec requires the fold be callable that way and that the `desk` example
does so. It does not require any other host to.

## Invariants and constraints

- One accepted utterance is exactly one row. A second `desk_post` in a turn
  replaces the first; it does not add a turn.
- Text outside a tool call becomes no row.
- A refused aside is a desk row plus a stated reason, never silence.
- `desk_close` appends its row and *then* closes: the host decides, and the
  message is never lost to the closing.
- The library holds no outbox, no notebook and no account. It holds the rules.
- The transcript stays append-only; nothing here edits a row.

## Acceptance criteria

1. `crates/tinyhivemind/examples/desk/mcp.rs` holds transport only — the
   JSON-RPC loop, the outbox file, and the CLI config block — and names no tool
   description, no schema and no validation rule of its own. **Met.**
2. Every tool description a seat reads comes from `speech::tool_specs`, asserted
   by a test that the example's served list equals it. **Met** —
   `serves_the_librarys_tool_surface_and_never_a_second_statement_of_it`.
3. A `desk_dm` naming a seat the aside policy declines produces a desk-visible
   row **and** a refusal the example prints and returns to the seat, exercised
   by a test at the library level and one in the example. **Met** — the MCP
   server is handed `--desk` and `--turn` so it can price the call while the
   seat can still act on the answer; without them it serves as before and only
   the seat is uninformed.
4. No code path converts a recipient id to text and re-parses it. **Met** —
   `speech::targets` builds `MentionTarget`s from the `to` field directly.
5. `extract_post` exists once, accepts both fences, and its wrong-but-plausible
   spelling is a named regression test. **Met** — `speech::fence`,
   `accepts_the_symmetric_closing_fence`.
6. Behavior on the run-28 path is unchanged: an offline desk run produces the
   same rows, audiences and dispatch decisions as before the refactor. **Met**
   — `crates/tinyhivemind/tests/utterance_surface.rs`.
7. `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features --
   -D warnings`, `cargo build --all-targets --all-features`,
   `cargo test --all-features` and `.github/scripts/assert-pure.sh` pass.
   **Met.**

## What this does not do

- **It does not make the room faster.** Cadence is
  [defect 3 of the desk lessons](../experiments/2026-09-09-desk-lessons.md) and
  is untouched here.
- **It does not add a continuously-contexted seat.** That is the larger bet the
  lessons rank first, and it is a separate spec.
- **It does not test the standing account.** Criterion 7 of
  [`thoughts-and-channels.md`](thoughts-and-channels.md) stays open.

## Open questions

- **Should `desk_read` be a port rather than a spec?** Reading the transcript
  waits on a log, so the *tool* is data here but its handler is host code
  against `SessionLog`. A thin `speech::read_request` fold that bounds `limit`
  would put the clamping in one place; it may not be worth a type.
- **Is `desk_close` one tool or a flag on `desk_post`?** Two tools cost a
  seat one more description to read; a flag risks being set by a seat that
  merely wants to sound finished. Run 28 argues for keeping it distinct and
  loudly described, and that is what is proposed, on one run's evidence.
- **What does the host do with a turn that calls nothing?** Silence is legible
  now, and the ladder's landing phase is the current answer. Whether a
  never-spoke turn should be *refused* and retried is unsettled.
