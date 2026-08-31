# 1. Record architecture decisions

- **Status:** Accepted
- **Date:** 2026-08-10

## Context

Significant technical decisions — the module boundaries, the error strategy,
the choice of a runtime or a dependency — are made once and then relied on for
years. Without a record, the reasoning is lost, and the decision gets
relitigated or silently reversed by whoever touches the code next. Commit
messages are too granular and pull request threads are too easy to lose.

## Decision

Record each significant architectural decision as a numbered file in
`docs/adr/`, using this document's structure: context, decision, consequences.

- Number files sequentially: `0002-...`, `0003-...`.
- An accepted ADR is immutable. To change a decision, write a new ADR and set
  the old one's status to `Superseded by <n>`.
- Statuses are `Proposed`, `Accepted`, `Superseded by <n>`, or `Rejected`.
- Write an ADR when a decision is hard to reverse, constrains future work, or
  will surprise a reader of the code. Routine implementation choices do not
  need one.

## Consequences

- New contributors can read why the system is shaped the way it is, in order.
- Reversing a decision is explicit and reviewable rather than incidental.
- There is a small ongoing cost: one short document per significant decision.

Copy this file as the template for the next ADR, and delete this one only if
the project genuinely does not want decision records.
