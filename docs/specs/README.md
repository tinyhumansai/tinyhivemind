# Specifications

Specifications define what the system must do before implementation details
take over. Create one for behavior that changes a public API, crosses module
boundaries, introduces a durable data format, or has meaningful operational
constraints.

Use a short kebab-case filename such as `retry-policy.md`. Each specification
should contain:

1. **Status and owner** — Draft, Accepted, Implemented, or Superseded.
2. **Problem** — the user or system need, without prescribing a solution.
3. **Goals and non-goals** — the exact boundary of the work.
4. **Proposed behavior** — public API, inputs, outputs, errors, and examples.
5. **Invariants and constraints** — properties every implementation must keep.
6. **Acceptance criteria** — externally observable pass/fail conditions.
7. **Open questions** — unresolved decisions that block acceptance.

After the specification is accepted, create a linked implementation plan in
[`../plans/`](../plans/README.md). Keep code snippets small enough to clarify
the contract; production code still belongs under `src/`.

See [`example-retry-policy.md`](example-retry-policy.md) for a complete sample.

## Accepted specifications

- [`chat-identity.md`](chat-identity.md) — the four stored spellings of the
  General conversation.
- [`desks.md`](desks.md) — host-owned desk overlays and the borrowed membership
  algebra.
- [`grammar.md`](grammar.md) — the index to the authoritative grammar
  reference for both textual grammars, with the code/prose discrepancies it
  resolved.
  - [`grammar-mentions.md`](grammar-mentions.md) — the complete `@` grammar:
    lexical rules, the alias table, resolution, normalization, and which
    mention wins for each consumer.
  - [`grammar-traces.md`](grammar-traces.md) — the complete `!marker` grammar:
    the eight kinds, the `#topic`, `>target` and `^cite` qualifiers, fence
    masking, and the markers that fail closed.
- [`mentions.md`](mentions.md) — roster records, mention grammar, normalization,
  and pure routing decisions.
- [`sessions.md`](sessions.md) — host-owned paging, attributed projection, and
  ephemeral team initialization.
- [`continuous-sharing.md`](continuous-sharing.md) — caller-owned watermarks
  and stateless attributed transcript deltas.
- [`responders.md`](responders.md) — deterministic one-responder selection and
  the model selector boundary.
- [`mention-dispatch.md`](mention-dispatch.md) — bounded one-target dispatch,
  the atomic host enqueue contract, and the sentences a refusal comes back in.
- [`cross-desk-referral.md`](cross-desk-referral.md) — one bounded child turn
  that may run on another channel, and the one answer that comes back.
- [`hive-mind.md`](hive-mind.md) — bounded group deliberation: traces, salience,
  quorum with cross-inhibition, and the attention market.
- [`refutation-and-grounds.md`](refutation-and-grounds.md) — a negative
  evidence-to-topic link, grounds weighed by evidential depth, and grounded
  objections.
- [`recall.md`](recall.md) — one selection ranking, the roster and desk
  pickers, bounded transcript search with optional regular expressions,
  pinning as a fold, and the stated per-message budget.
- [`expert-delegation.md`](expert-delegation.md) — a transactive-memory
  directory folded from grounded deposits and the citations they drew,
  `BidReason::Knows`, and `!defer`.

## Draft and proposed specifications

- [`thread-scoped-conversations.md`](thread-scoped-conversations.md) — proposed:
  which half of OpenCompany's threads epic this layer owns, and which stays with
  the host.
- [`shared-medium-schema.md`](shared-medium-schema.md) — draft: what a projected
  message carries, per-conversation read state, digests, and supersession.
- [`approval.md`](approval.md) — proposed: a pure gate for a side-effecting
  action — `approve` as a total fold, standing grants as a liveness and
  coverage predicate, and epoch-scoped consent that cannot apply backwards.
  - [`approval-testing.md`](approval-testing.md) — the full one-test-per-
    failure-path list, split out to keep the spec itself under the per-file
    line budget.
- [`private-asides.md`](private-asides.md) — draft: an audience on a stored row
  and a viewer on a query, so two agents on one desk can compare notes without
  the desk reading them; what a non-member sees instead, and what the exchange
  owes the room when it ends.
- [`seat-continuity.md`](seat-continuity.md) — draft: the private, superseding
  notebook a seat carries between turns, the one feedthrough row a turn that
  wrote files leaves behind, and the brief appended once. First slice lives in
  the `desk` example.
- [`folding-by-size.md`](folding-by-size.md) — draft: the standing account
  folds when the room is large rather than only when it is long, stated as a
  character budget a host writes as a token budget, so a seat joining a big
  room is always handed an account of it.
- [`the-utterance-surface.md`](the-utterance-surface.md) — draft: the algebra of
  what a seat says moves out of the `desk` example and into
  `tinyhivemind::speech` — the tool descriptions as data, one fold from an
  utterance to a row, and a policy refusal that reaches its author while the
  turn is still running. The mention grammar keeps the routing.
- [`thoughts-and-channels.md`](thoughts-and-channels.md) — draft: a seat's
  output text is its own thinking and it reaches the room through tools with a
  schema (`desk_post`, `desk_dm`, `desk_read`); a channel carries one bounded,
  superseding account of everything older than its live tail, folded only from
  rows every member may read. The library half is `tinyhivemind::digest`.

## Decisions these specifications rest on

An accepted specification cites the record that settled its contested question
rather than restating it. Two run across several specifications:

- [`off-floor-exchange.md`](off-floor-exchange.md) — draft: private exchange
  in rounds between turns, taking no floor and bounded by a host-set budget.
- [ADR 0008](../adr/0008-an-approval-decision-is-total.md) — an approval
  decision denies rather than fails, so a gate cannot be bypassed by failing.
- [ADR 0009](../adr/0009-a-refusal-renders-what-the-caller-already-holds.md) —
  a refusal renders only what the caller already holds. The library owns the
  words, and every reason that turns on a named other collapses to one shared
  sentence, so a set of refusals cannot be probed for a roster. It settles what
  [`mention-dispatch.md`](mention-dispatch.md), [`responders.md`](responders.md)
  and [`approval.md`](approval.md) each left open.
