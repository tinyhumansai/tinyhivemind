//! Unit tests for the greeting module.
//!
//! Unit tests live next to the code they cover and may reach into private
//! items. Tests of the public contract belong in `tests/` instead.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

#[test]
fn greets_a_named_person() {
    assert_eq!(greet("Ferris").unwrap(), "Hello, Ferris!");
}

#[test]
fn trims_the_name() {
    assert_eq!(greet("  Ferris  ").unwrap(), "Hello, Ferris!");
}

#[test]
fn rejects_an_empty_name() {
    assert_eq!(greet("").unwrap_err(), Error::EmptyName);
}

#[test]
fn rejects_a_whitespace_only_name() {
    assert_eq!(greet(" \t\n ").unwrap_err(), Error::EmptyName);
}
