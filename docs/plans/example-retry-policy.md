# Example plan: Retry policy

- **Status:** Example
- **Specification:**
  [`../specs/example-retry-policy.md`](../specs/example-retry-policy.md)

> This is a sample implementation plan, not active work. Replace or remove it
> when generating a real project from this template.

## Goal

Add the specified typed retry policy through small red-green-refactor steps,
without adding a runtime, timers, or new dependencies.

## Task 1: Add the constructor contract

**Files:** `src/retry/mod.rs`, `src/retry/types.rs`, `src/retry/test.rs`

1. Create the module skeleton and a failing test for zero attempts:

   ```rust
   #[test]
   fn rejects_zero_max_attempts() {
       assert_eq!(
           RetryPolicy::new(0).unwrap_err(),
           Error::ZeroMaxAttempts,
       );
   }
   ```

2. Add `Error::ZeroMaxAttempts` in `src/error/mod.rs` and its message assertion
   in `src/error/test.rs`.
3. Implement `RetryPolicy::new` using `NonZeroU32`, keeping the field private.
4. Run `cargo test retry` and `cargo clippy --all-targets --all-features -- -D warnings`.

## Task 2: Add attempt-boundary behavior

**Files:** `src/retry/mod.rs`, `src/retry/test.rs`

1. Add failing tests for attempts `0`, `1`, the maximum, and one past it.
2. Implement the smallest boundary check:

   ```rust
   #[must_use]
   pub fn allows_attempt(self, attempt: u32) -> bool {
       attempt != 0 && attempt <= self.max_attempts.get()
   }
   ```

3. Run `cargo test retry`.

## Task 3: Publish and document the API

**Files:** `src/lib.rs`, `tests/public_api.rs`, `README.md`

1. Re-export `RetryPolicy` from `src/lib.rs`.
2. Add an integration test using only `template::{Error, RetryPolicy}`.
3. Add a runnable README example and rustdoc `# Errors` documentation.
4. Run `cargo test --doc` and `cargo test --test public_api`.

## Task 4: Full verification

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] `cargo build --all-targets --all-features`
- [ ] `cargo test --all-features`
- [ ] `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features`
- [ ] `cargo deny check all`

When all checks pass, mark the specification Implemented and replace this
example status with the actual completion state.
