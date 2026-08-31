# Implement continuous transcript sharing

Implements [`../specs/continuous-sharing.md`](../specs/continuous-sharing.md).

## Goal and boundaries

Add a stateless P5 delta planner over P4's host log and page validation. Keep
state caller-owned, reuse attributed messages, and implement no persistence,
CAS primitive, responder selection, or mention dispatch.

## Test-first tasks

1. Add `sharing/types.rs` tests that pin value payload serde and conversation
   equivalence, then define state, query, delta, plan, and reason records.
2. Add failing pure tests for initialized state and bounded, idempotent
   `note_present`, then implement them with typed overflow errors.
3. Add failing tests for changed conversations, regressing/equal bounds, and
   thread/channel identity, then implement early planning decisions.
4. Add failing one-page and multi-page tests for exclusivity, chronological
   attribution, interleaved peer rows, General aliases, presence omission, and
   future-presence retention; reuse P4 page matching and validation helpers.
5. Add failing sparse-page, watermark exhaustion, scan-cap, read-error, and
   malformed-page tests; implement bounded termination with no partial state.
6. Add retry and simulated host-CAS tests demonstrating that only the caller
   commits returned next state after accepting the delta and trigger.
7. Add module docs, README, centralized exports, public API tests, indexes, and
   roadmap updates, then run focused and full verification plus coverage.

## Completion checklist

- [x] State and presence bookkeeping are caller-owned and bounded.
- [x] Delta planning is exclusive, chronological, attributed, and stateless.
- [x] Conversation identity, failures, and reinitialization are typed.
- [x] Retry and host commit timing are tested and documented.
- [x] P4 validation is reused rather than forked.
- [x] Full workspace verification passes.
