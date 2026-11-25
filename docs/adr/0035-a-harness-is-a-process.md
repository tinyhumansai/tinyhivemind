# 35. A harness is a process

- **Status:** Accepted
- **Date:** 2026-11-25
- **Specification:** [harnesses](../specs/harnesses.md)
- **Amends:** [ADR 0034](0034-the-hive-language-defines-the-deployment.md), in
  that the language's vocabulary is no longer exactly what OpenHuman runs
- **Supersedes, when the link is removed:**
  [ADR 0020](0020-openhuman-embed-is-a-git-dependency-patched-locally.md), and
  the single linking crate of [ADR 0028](0028-three-crate-workspace.md)

## Context

The OpenHuman adapter links the harness. A consumer must unify OpenHuman types
across its own checkout, copy a long set of `[patch]` tables, and run everything
in one process. Two other harnesses are wanted in the same hive. OpenClaw's
agent runtime is not published for embedding; its documented embedding path is a
supervised Gateway process. Codex is a program. Neither can be linked.

[The spike](../experiments/2026-11-25-process-harnesses.md) ran all three as child
processes against a scripted model. Each called a hive tool as the right seat,
kept its session across a restart, reported usage and cancelled.

## Decision

- Every harness runs as a supervised child process behind one port in a new
  `tinyhivemind-host` crate. `AgentRunner` stays the coordinator's seam.
- The kinds are `openhuman`, `openclaw` and `codex`. Each has a client crate that
  links no harness.
- A seat chooses its harness through the package; hives may be mixed. The package
  never names an executable. A per-kind capability table decides what `check`
  accepts.
- The linked OpenHuman path stays until the removal gate passes: each thing a
  package can say today either works on the process harness or is rejected by
  `check`. ADRs 0020 and 0028 hold until that change lands.
- A permission that cannot be enforced is rejected. Built-in tools start off.

## Consequences

- Consumers stop building OpenHuman and stop unifying its types. In exchange they
  deploy pinned binaries and own more process supervision.
- Public API breaks at removal: `AgentFactory`, `TurnHooks` over OpenHuman types,
  host session stores, injected providers and `HiveMemory`. `rsi-ralph` uses
  these and may stay on the last linked commit.
- In-process hooks on OpenHuman built-in tools have no equivalent. Packages that
  need them cannot move until OpenHuman exposes one.
- Memory cost is per process and varies by more than an order of magnitude;
  OpenClaw is therefore shared per deployment by default.
- Harness releases become a runtime compatibility surface, checked at start.
- Codex's app-server and parts of OpenClaw's recovery behaviour carry no
  stability promise. Pins and conformance tests are the mitigation.
