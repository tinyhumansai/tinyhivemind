# Desk tool tests

Unit tests for `../mcp.rs`: the room as a tool a seat calls, rather than a
fence it writes.

## Files

- `test.rs` — the outbox round trip (a post and a dm drained in the order they
  were said, with `@` stripped, and a cleared outbox holding nothing from the
  turn before); that a garbled line is skipped rather than losing the turn;
  that the MCP block merges into an existing agent configuration and survives
  one that is missing or unparseable; that exactly three tools are listed and
  each carries a schema; that a call with no message or no recipient is
  refused and leaves no utterance behind; and that `read` shows only what
  every member may read, chronologically.
