//! Unit tests for the crate-wide error type.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

#[test]
fn renders_a_human_readable_message() {
    assert_eq!(Error::EmptyName.to_string(), "name must not be empty");
}

#[test]
fn is_a_standard_error() {
    fn assert_error<E: std::error::Error>(_: &E) {}

    assert_error(&Error::EmptyName);
}
