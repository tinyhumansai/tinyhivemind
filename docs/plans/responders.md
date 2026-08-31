# Implement responder selection

Linked specification: [`../specs/responders.md`](../specs/responders.md)

## Goal

Add the pure responder ladder and the narrow async selector boundary without
starting turns or exposing host state.

## Test-first tasks

1. Add failing desk wire tests for default lead compatibility and auto mode;
   implement `ResponderMode` and update desk examples and fixtures.
2. Add failing pure tests for each responder rung, effective candidate
   construction, ambiguity, and orchestrator fallback; implement the responder
   payloads, `responder_plan`, and typed structural failures.
3. Add failing acceptance tests for strict selector output; implement
   `accept_selection`.
4. Add failing runtime tests for one selector call, success, absence, failure,
   and invalid output; implement the object-safe `Selector` port and
   `choose_responder` without a dispatch API.
5. Export and document the APIs, link the spec and plan, and mark P6 done/P7
   next in `ROADMAP.md`.

## Verification

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --all-targets --all-features
cargo test --all-features
.github/scripts/assert-pure.sh
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
cargo test --doc
```

## Completion checklist

- [x] Specification accepted before implementation.
- [x] Lead-compatible and auto desk wire forms tested.
- [x] Every pure ladder rung and strict output acceptance tested.
- [x] Selector boundary tested without dispatch capability.
- [x] Public docs, indexes, and roadmap updated.
- [x] Full verification passes.
