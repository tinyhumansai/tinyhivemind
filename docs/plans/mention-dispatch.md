# Implement mention dispatch

Linked specification: [`../specs/mention-dispatch.md`](../specs/mention-dispatch.md)

## Goal

Add a pure, bounded one-target dispatch decision and one atomic host enqueue
port without adding persistence, retries, fan-out, or host types.

## Test-first tasks

1. Add failing core wire and decision tests; implement dispatch payloads,
   reasons, policy ordering, first-direct-mention fail-closed behavior, and
   checked child-hop construction in `crates/tinyteams-core/src/dispatch/`.
2. Add failing runtime port tests for no-call, one-call, refusal, failure,
   bound scope, and concurrent idempotency; implement
   `MentionTurnQueue` and `dispatch_mention` in
   `crates/tinyteams/src/dispatch/`.
3. Add typed runtime enqueue failure, public exports and crate documentation.
4. Link the specification and plan, then mark P7 complete with host integration
   and live verification as the next work.

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
- [x] Pure dispatch decisions and wire forms tested.
- [x] Atomic queue boundary and refusal/error mapping tested.
- [x] Exactly-once behavior tested under concurrent duplicate calls.
- [x] Public docs, indexes, and roadmap updated.
- [x] Full verification passes.
