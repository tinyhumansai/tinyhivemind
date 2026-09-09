# Back the desk example with tinyinference and tinytools

**Status:** Landed. `anyhow` came with `tinytools` — implementing its `Tool`
means naming the error type it returns — and `--tool-surface` was added so the
`tinytools` rendering has a caller in this host rather than only in its tests.

Linked decision: [`../adr/0013-a-vendored-crate-is-an-example-dependency.md`](../adr/0013-a-vendored-crate-is-an-example-dependency.md)
Linked specification: [`../specs/the-utterance-surface.md`](../specs/the-utterance-surface.md)

## Goal

Replace the two things the `desk` example does by hand — reaching a model by
shelling out to `curl`, and declaring a tool surface inline — with the crates
that exist for each: `tinyinference` for the provider layer behind the
`Digester` and wrap-up paths, and `tinytools` for rendering the tool surface a
seat calls.

Both arrive as `[dev-dependencies]` git dependencies pinned by revision. No
library crate gains a dependency and `assert-pure.sh` is unchanged.

## Assumptions

- [`promote-the-utterance-surface.md`](promote-the-utterance-surface.md) lands
  first. This plan renders `speech::tool_specs()`; it does not define it, and
  doing this work before that one would put the descriptions in a third place.
- `tinytools::Tool` is async and needs `async-trait`; `tinyinference` needs a
  `tokio` runtime. The example already declares `tokio` as a dev-dependency.

## Test-first tasks

1. Add the two git dev-dependencies to the root `[workspace.dependencies]`,
   pinned by `rev`, each with the comment the repository's dependency guidance
   asks for: what it is, and which example uses it. Take them in
   `crates/tinyhivemind/Cargo.toml` under `[dev-dependencies]`. Then run
   `.github/scripts/assert-pure.sh` and confirm it still reports clean — that
   run is the test for this task, and it is what proves the boundary held.

2. Add a failing test in `examples/desk/chat/test.rs` that a completion answers
   from a `tinyinference` mock model, then replace `chat::Chat`'s `curl`
   subprocess with a `tinyinference` `ChatModel`. Keep the public shape
   (`Chat::new`, `Chat::complete`) so `digest.rs` and the wrap-up rung do not
   move. Carry across the two hard-won behaviors the current client documents:
   no `max_tokens` (a reasoning model spends the cap on reasoning and returns
   `finish_reason: length` with empty content), and the timeout.

3. Add failing tests for provider failure classification — a refusal, a
   timeout, and a 5xx must be distinguishable — then map `tinyinference`'s
   normalized failures onto what `turn.rs`'s ladder already branches on. This is
   the real gain: the ladder currently reads "did anything come back" because
   `curl` gives it nothing better, and
   [defect 1 of the desk lessons](../experiments/2026-09-09-desk-lessons.md) is
   an unexplained hang that better failure information may well name.

4. Add a failing test that `RoomDigester` produces an account through the mock
   model, then point it at the new `Chat`. The `Digester` impl itself does not
   change — that is the point of the port and worth asserting by the diff being
   empty.

5. Add a failing test that every `speech::tool_specs()` entry round-trips into a
   `tinytools::Tool` with the same name, description and schema, then implement
   `examples/desk/tools.rs`: one adapter type over a `ToolSpec` plus a handler,
   whose `execute` appends to the outbox exactly as the MCP handler does. Assert
   MCP and `tinytools` render the *same* list, so a seat sees one surface
   whichever way the host runs it.

6. Add a failing test that a `desk_dm` refused by the aside policy comes back as
   a `ToolResult` error carrying the reason, and implement it against
   `commit_utterance`. This is the `tinytools` path's half of criterion 3 of
   [`../specs/the-utterance-surface.md`](../specs/the-utterance-surface.md).

7. Update `examples/desk/README.md` and `crates/tinyhivemind/examples/README.md`
   to say what each crate is for and that neither is a library dependency, and
   link ADR 0013.

## Validation

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
cargo test --all-features
.github/scripts/assert-pure.sh
tree="$(cargo tree -p tinyhivemind -e normal,build)" || exit 1
if printf '%s\n' "$tree" | grep -Eiq 'reqwest|anyhow|tokio'; then
  echo "unexpected library dependency" >&2
  exit 1
fi
echo clean
```

The last command is the one that matters and should print `clean`: the library
tree must not move. `assert-pure.sh` above is the actual enforced CI gate;
this command is the same spot-check written to fail loudly — a bare
`grep ... || echo clean` prints `clean` whenever `grep` finds no match, which
also happens to be what it prints if `cargo tree` itself fails, so a stricter
form is worth carrying here even though nothing failed when this plan landed.

## What this plan deliberately leaves out

- **An in-process agent loop.** Rendering the tool surface through `tinytools`
  makes one possible; building one is a separate question and would change what
  a turn *is*.
- **Streaming.** `tinyinference` has it and the wrap-up and the fold are both
  one-shot completions that do not need it.
- **Retiring `curl` elsewhere.** `examples/desk/memory.rs` also shells out; it
  talks to a different service and is out of scope here.
