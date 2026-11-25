# 36. Hive tools are served over loopback MCP

- **Status:** Accepted
- **Date:** 2026-11-25
- **Specification:** [harnesses](../specs/harnesses.md)
- **Amends:** [ADR 0027](0027-retire-unused-adapters-and-optional-mechanisms.md),
  which removed `tinyhivemind-mcp` and asked that a return carry a new
  specification and measurements

## Context

ADR 0027 removed the MCP server because the one harness had native tools, making
it a second binding and a listener with no role, and because the live record
showed tool discovery and call failures. It said those runs did not show MCP
unsuitable elsewhere. [ADR 0035](0035-a-harness-is-a-process.md) changes the
premise: a child process cannot be handed a native Rust tool.

[The spike](../experiments/2026-11-25-process-harnesses.md) measured it. MCP over
HTTP with a per-seat bearer reached the right actor on all three harnesses. The
earlier discovery failures have identifiable counterparts: OpenClaw and Codex hid
MCP tools behind tool search by default, and OpenHuman's first turn ran before
its tool cache was warm. Each has a setting or a check that fixed it.

Two findings shape the server. OpenHuman with a wildcard allowlist let one seat
call through another seat's server. OpenClaw resumes an interrupted turn after a
crash, so a correct bearer does not prove that a call belongs to the current turn.

## Decision

- A new `tinyhivemind-mcp` crate serves the hive vocabulary and package tools over
  Streamable HTTP on 127.0.0.1, on a random port.
- The bearer, one per seat, decides the actor. No argument does.
- A state-changing call names its turn, and is refused unless that turn is the
  seat's running one.
- The server offers tools only, and enforces package hooks, permissions and send
  policy before dispatch.
- It holds no episode state. The coordinator remains the only record, and nothing
  pure depends on this crate.
- Codex's experimental `dynamicTools` served a hive tool over stdio with no
  listener. It is not adopted: it covers one harness and is experimental. Revisit
  if it stabilises.

## Consequences

- The repository opens a listening socket again, on loopback only.
- The bearer sits in each instance's private config. A shared OpenClaw instance
  holds several seats' bearers.
- One server gives all three harnesses the same policy point, which is what lets a
  package's hooks and permissions survive the move out of process.
- Every harness must prove at start that the model sees exactly the expected
  tools, because their defaults did not.
