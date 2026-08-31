# Example: Retry policy

- **Status:** Example
- **Owner:** Maintainers
- **Plan:** [`../plans/example-retry-policy.md`](../plans/example-retry-policy.md)

> This demonstrates the expected specification format. Replace or remove it
> when generating a real project from this template.

## Problem

Callers need a typed way to limit retries without duplicating attempt counting
and validation. The crate currently has no retry behavior.

## Goals

- Expose an immutable retry policy with a non-zero maximum attempt count.
- Let callers determine whether another attempt is permitted.
- Reject zero attempts through the crate-wide error type.

## Non-goals

- Sleeping, backoff, jitter, or executing operations.
- Deciding which application-specific errors are retryable.
- Persisting retry state.

## Proposed behavior

The public surface is deliberately small:

```rust
use template::{RetryPolicy, Result};

fn policy() -> Result<RetryPolicy> {
    let policy = RetryPolicy::new(3)?;
    assert!(policy.allows_attempt(1));
    assert!(!policy.allows_attempt(4));
    Ok(policy)
}
```

`RetryPolicy::new(0)` returns a dedicated `Error::ZeroMaxAttempts` variant.
Attempt numbers are one-based: attempt `1` is the initial call, not the first
retry.

## Invariants and constraints

- `max_attempts` is always greater than zero after construction.
- `allows_attempt(n)` is true exactly when `1 <= n <= max_attempts`.
- The type is cheap to copy and does not perform I/O or observe time.
- New public items have rustdoc and are re-exported from `src/lib.rs`.

## Acceptance criteria

- Construction succeeds for `1` and `u32::MAX` and fails for `0`.
- Boundary checks cover attempts `0`, `1`, `max_attempts`, and
  `max_attempts + 1` when representable.
- Integration tests prove the policy and its error are available to consumers.
- Formatting, Clippy, build, tests, rustdoc, and cargo-deny pass.

## Open questions

None for this example.
