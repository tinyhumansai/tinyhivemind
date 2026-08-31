//! Integration tests for the public crate surface.
//!
//! These tests link against the crate as a downstream consumer would: they can
//! only use what `src/lib.rs` re-exports. Treat them as the regression suite
//! for the crate's public contract — if a change breaks a test here, it is a
//! breaking change for users.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use template::{Error, greet};

#[test]
fn greeting_is_available_to_consumers() {
    assert_eq!(greet("Rust").unwrap(), "Hello, Rust!");
}

#[test]
fn errors_are_available_to_consumers() {
    assert_eq!(greet("").unwrap_err(), Error::EmptyName);
}
