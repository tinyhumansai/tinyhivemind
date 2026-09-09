# `speech`

The room as a tool: what a seat may say, what makes a call valid, and what one
accepted utterance becomes.

## Why it exists

A seat's free text is its thinking. What it says to the room is an *action*,
and before this module that action was a delimiter the model wrote into its own
output. A model-authored delimiter is an interface with a fallible producer: a
seat closed a verified result with `<<<POST>>> … <<<POST>>>` instead of
`<<<POST … POST>>>`, the room received the three characters `>>>`, read that as
silence, and nudged the seat that had just spoken.

A tool call has a schema, a validator, and a way to tell its producer it got it
wrong *inside the turn that got it wrong*. That is the whole argument, and it is
worth more than the run that motivated it: a delimiter can only be mis-parsed
after the fact.

## Files

| file | holds |
| --- | --- |
| `mod.rs` | `interpret`, `commit_utterance`, `check_recipients`, `addressed_peers`, `read_limit` |
| `types.rs` | `Utterance`, `ToolCall`, `UtteranceRejection`, `CallArguments`, `CommittedUtterance`, `ParameterKind` |
| `tools.rs` | `tool_specs()` — the four tools, their descriptions and their arguments, as data |
| `fence.rs` | `extract_post`, for the two callers that cannot reach a tool |
| `test/` | `parse`, `commit`, `tools`, `fence`, and the shared `support` fixture |

## The surface

- **`interpret(name, arguments)`** reads one call and returns a `ToolCall`, or
  the `UtteranceRejection` to hand back to the seat. It parses no JSON: a host
  maps its own wire onto `CallArguments`.
- **`commit_utterance(request)`** is the fold from an accepted utterance to the
  row a host appends — content, `Audience`, mentions, `closing`, and the reason
  a requested aside was declined.
- **`tool_specs()`** states each tool once. The descriptions are contract text,
  not documentation: they are the only place a seat is told that what it writes
  outside a call reaches nobody. A host renders them verbatim.

## Constraints worth knowing

- **A tool call is a request to speak.** Nothing here appends, and nothing here
  waits. The host appends, and the host decides.
- **A refused aside is a desk row.** The refusal travels beside the audience
  rather than in place of it, so the message reaches the room either way. That
  is the safe direction to fail in, and the caller can still say which way it
  went.
- **A `dm`'s recipients are targets, not prose.** They are built from the `to`
  field directly. Spelling them back into `@a @b` and re-reading them through
  the mention grammar loses any id the grammar does not accept, silently — and
  the grammar's code-span masking means a body can swallow one.
- **The grammar keeps the routing.** A tool says who may *read* a message; the
  first `@id` in the body still says whose turn is next. Most of what routes in
  a room — an operator brief, a person's message, a chair's nudge — cannot call
  a tool, so routing stays text-borne.
- **The bookkeeping is the host's.** `spent` and `unsettled` are folded from
  the host's own journal, because neither is derivable from arguments this
  crate is given.

## Reading

- [`docs/specs/the-utterance-surface.md`](../../../../docs/specs/the-utterance-surface.md)
- [`docs/specs/thoughts-and-channels.md`](../../../../docs/specs/thoughts-and-channels.md)
- [`docs/experiments/2026-09-08-pe1006-tool-room.md`](../../../../docs/experiments/2026-09-08-pe1006-tool-room.md)
