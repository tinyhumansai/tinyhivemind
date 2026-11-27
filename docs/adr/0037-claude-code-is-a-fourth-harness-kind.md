# 37. Claude Code is a fourth harness kind

- **Status:** Proposed
- **Date:** 2026-11-26
- **Specification:** [harnesses](../specs/harnesses.md)
- **Amends:** [ADR 0035](0035-a-harness-is-a-process.md), whose kinds were
  `openhuman`, `openclaw` and `codex`

## Context

The process harness work aims at one hive whose seats run on different
harnesses. Claude Code is a harness a team is likely to want there: it runs
headless as a command-line program, streams newline-delimited JSON over
stdio, takes MCP servers with request headers, keeps sessions it can resume,
and can be held to an exact tool allowlist. It was not in the November spike,
so nothing about it is measured yet.

It differs from the other three in one way the language must see: it speaks
only Anthropic's Messages API. A package that gives a `claude-code` seat an
`openai` endpoint would be accepted and then fail every turn.

## Decision

- `claude-code` is a harness kind, beside `openhuman`, `openclaw` and `codex`,
  with one instance per seat.
- Its model API is Anthropic's Messages API (`ModelApi::Messages`). A new
  capability row, provider `anthropic` with an explicit `endpoint` and no
  `credential`, is the one provider shape it accepts; `openai` with an
  endpoint is rejected for it by the spec. The other kinds reject the new row.
- Every other row is the same as `codex`'s: built-in tools off, the rest
  rejected until a conformance test on the pinned release proves it.
- The client is `tinyhivemind-claude-code`, a crate that links no harness, as
  ADR 0035 requires of every client.

## Consequences

- `HarnessKind::ALL` and every capability row grow a column; a package naming
  `claude-code` is checked like any other.
- The key rule is unchanged: a seat gets an endpoint, never a credential, so a
  real Anthropic model is reached through a host-side proxy that holds the key.
- The pin, the start-up verification and the conformance suite carry the
  evidence the spike carried for the other three. Until they pass on the
  pinned release, this decision stays proposed.
