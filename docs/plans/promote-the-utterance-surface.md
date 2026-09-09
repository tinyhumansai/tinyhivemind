# Promote the utterance surface into the library

**Status:** Landed.

Linked specification: [`../specs/the-utterance-surface.md`](../specs/the-utterance-surface.md)

## Goal

Move the algebra of *what a seat says and what that becomes* out of
`crates/tinyhivemind/examples/desk` and into `crates/tinyhivemind::speech`, so a
host takes it as a dependency rather than reimplementing it — and close the two
defects the move exposes: a policy refusal that never reaches its author, and a
recipient list that is turned back into prose to be re-parsed.

No new port, no transport in the library, no change to the mention grammar or to
one-message-one-turn.

## Test-first tasks

1. Add failing tests for the utterance wire form and its rejections in
   `crates/tinyhivemind/src/speech/test/parse.rs`, then implement
   `crates/tinyhivemind/src/speech/`: `Utterance`, `UtteranceRejection`, and
   `parse_utterance`. Port the cases the example already covers in
   `examples/desk/mcp/test.rs` — a blank message, a `dm` with an empty `to`, a
   `to` entry written `@lead`, an unknown kind, a garbled line — and add the
   ones it does not: an id that is not in the roster, and a `to` naming the
   speaker itself. Pin the serde representation, since the outbox is a wire the
   host writes and the host reads.

2. Add a failing test that the served tool list equals `speech::tool_specs()`,
   then move the four descriptions and schemas out of `examples/desk/mcp.rs`
   into `speech::tool_specs`. The descriptions are contract text, not
   documentation: keep them verbatim so run 28's prompt is unchanged, and let
   the test in the example assert equality rather than restating them.

3. Add failing tests for `commit_utterance` in
   `crates/tinyhivemind/src/speech/test/commit.rs` covering: a `Post` becoming
   one desk row with its mentions resolved; a `Dm` becoming an aside whose
   audience came from the `to` field and not from the message body; a `Dm` the
   aside policy declines becoming a desk row **with** a populated
   `refusal`; a `Close` carrying `closing: true` with its row intact; and a
   `Dm` whose recipient id would not survive the mention grammar — the case
   the current string round trip loses silently. Then implement
   `CommitRequest`, `CommittedUtterance` and `commit_utterance` as a fold over
   `mention::resolve`, `aside::address` and the roster.

4. Add a failing test that a `Dm` naming an ineligible recipient yields a
   refusal *before* any append, then rewire `examples/desk/run.rs` to call
   `commit_utterance` in place of its resolve/format/re-resolve/`aside::address`
   block. Delete the `format!("@{id}")` round trip. The printed
   `aside to @…` line and the appended row must be identical to today's for the
   accepting case — task 9 is what proves it.

5. Add a failing test in the example that a `desk_dm` whose aside is refused
   returns the reason as the tool's own result, then implement it: the MCP
   handler calls `commit_utterance` against the roster and desks it already
   holds, and answers `isError` with the reason. The row is still appended
   desk-visible afterwards, so a seat that ignores the refusal loses nothing.

6. Move `extract_post` and its fence handling into `speech::fence` with both
   spellings, and carry over the run-26 regression under a name that says what
   it is (`accepts_the_symmetric_closing_fence`). Point the example's tool-less
   wrap-up rung at it and delete the local copy.

7. Reduce `examples/desk/mcp.rs` to transport: the JSON-RPC loop, `serve`,
   `config_block`, the outbox `clear`/`drain`, and nothing that decides
   anything. Assert the reduction by grep in review, not by a test.

8. Export `Utterance`, `UtteranceRejection`, `ToolSpec`, `tool_specs`,
   `CommitRequest`, `CommittedUtterance` and `commit_utterance` from
   `crates/tinyhivemind/src/lib.rs`, and give `speech/` a `README.md` covering
   the split between what the library decides and what the host serves.

9. Add an offline end-to-end regression: run the desk example's fold over a
   fixed script of drained utterances and assert the produced rows, audiences,
   `closing` flag and dispatch decisions match a checked-in expectation
   captured from the pre-refactor code. This is the acceptance criterion that
   the refactor changed no behavior on the run-28 path.

10. Update the docs in the same change: mark
    [`../specs/thoughts-and-channels.md`](../specs/thoughts-and-channels.md)'s
    implementation note to say the surface is library-side, add the phase row to
    `ROADMAP.md`, and note in
    [`../experiments/2026-09-09-desk-lessons.md`](../experiments/2026-09-09-desk-lessons.md)'s
    "what to build next" list that item 5 — every model-authored delimiter is a
    fallible interface — is discharged for the room's own delimiters by this
    work and still open for `!pin` and `!surface`.

## Validation

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
cargo test --all-features
.github/scripts/assert-pure.sh
```

Then one live run on PE 1006 with the same flags as run 28, to confirm the
refusal path fires at least once and that per-turn `read` counts stay where run
28 put them.

## What this plan deliberately leaves out

- **Cadence.** 19.4 → 4.4 minutes per message was run 28's headline and nothing
  here improves it further.
- **A continuously-contexted seat.** Ranked first in the desk lessons and a
  larger design question; it needs its own spec.
- **Testing the standing account.** It has still never folded in a live run.
- **The other delimiters.** `!pin`, `!aside` and `!surface` keep their
  fallible-producer problem until they are given the same treatment.
