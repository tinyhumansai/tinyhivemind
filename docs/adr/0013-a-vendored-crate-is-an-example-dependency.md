# 13. A vendored crate may back an example and never a library crate

- **Status:** Proposed
- **Date:** 2026-09-09

## Context

Two crates in this workspace's own ecosystem answer questions the `desk` example
currently answers by hand.

`tinytools` (`tinyhumansai/tinytools`) is the tool vocabulary — the `Tool`
trait, `ToolResult`, and the classifications a host enforces around a call. Its
stated reason for existing is that a harness and a host both have to name a
tool's result, and when each declares its own the conversions between them get
written by hand at every seam. The `desk` example writes exactly those
conversions: `examples/desk/mcp.rs` declares four tools' names, descriptions and
JSON schemas inline and hand-rolls their dispatch.

`tinyinference` (`tinyhumansai/tinyinference`) is the provider layer — a
`ChatModel` abstraction with streaming, OpenAI-shaped and OpenAI-compatible
adapters, normalized provider failures and retry classification. The `desk`
example reaches a model by **shelling out to `curl`** with a hand-written
request body (`examples/desk/chat.rs`), which is what the `Digester` port and
the tool-less wrap-up rung both run on.

So both are wanted. The question this decision answers is *where they may go*.

The repository's guidance has said there are no vendored dependencies, and the
reason is sound: this repository is itself vendored — a consumer pins it as a
submodule and takes `crates/*` as path dependencies — so anything it vendored in
turn becomes a nested submodule in every consumer.

That reason is about the **library crates**. It does not reach an example.

## Decision

A crate from outside this workspace may be a **dev-dependency used only by an
example**. It may never be a dependency of `tinyhivemind-core`,
`tinyhivemind-hive`, or `tinyhivemind`.

Concretely:

- `tinytools` and `tinyinference` are declared under `[dev-dependencies]` in
  `crates/tinyhivemind/Cargo.toml`, as **git dependencies pinned by revision**.
  They are not submodules, so `.gitmodules` is unchanged and a consumer
  initializing recursively gains nothing new.
- A consumer takes `crates/*` as path dependencies and never builds the
  example, so it never resolves either crate. `cargo build -p tinyhivemind`
  does not fetch them; `cargo test --all-features` does.
- `.github/scripts/assert-pure.sh` is unchanged and still passes, because it
  reads `cargo tree -e normal,build`, which excludes dev-dependencies. This is
  not a loophole being exploited: it is the same boundary, asserted the same
  way. The library's dependency tree is what the script guards and the library's
  dependency tree does not move.

Both crates are, independently, ones the library could not take. `tinytools`
depends on `anyhow`, which this repository forbids in every crate in favor of a
crate-wide error enum. `tinyinference` depends on `reqwest`, `tokio` and
`futures`, and `reqwest` is forbidden even for the runtime crate, whose ports
exist precisely so that a transport stays on the host's side of them.

## Consequences

**The ports keep doing their job.** `Digester` and `Selector` take a request and
return text, with no host handle and no callback. Backing them with a real
provider crate changes the example and changes nothing about the seam, which is
the evidence that the seam was drawn in the right place.

**The tool surface has one statement and two renderers.** `speech::tool_specs()`
(see [`../specs/the-utterance-surface.md`](../specs/the-utterance-surface.md))
states each tool as data; the example renders that data into MCP today and into
`tinytools::Tool` implementations for a host that runs an agent loop in-process.
Neither renderer restates a description.

**A pinned revision is a maintenance obligation.** Two git dependencies pinned
by rev will go stale, and nothing in CI will say so. That is accepted: they back
an example, and an example that fails to build is a visible failure rather than
a silent one.

**The example gains a build cost.** `cargo test --all-features` now compiles
`reqwest` and `tokio`'s full tree for the example's sake. Measured against
`curl` in a subprocess with a hand-written body and no error classification,
that is a trade worth making — but it is a real cost and it lands on every
contributor.

**If either crate is ever needed by a library crate, this decision is wrong
rather than bent.** The answer then is a port, as it has been every other time.
