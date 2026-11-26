//! Bearers: random, compared exactly, and never printed.
#![allow(clippy::expect_used)]
use super::Bearer;

#[test]
fn a_bearer_is_64_hex_characters_and_matches_only_itself() {
    let bearer = Bearer::generate().expect("randomness");
    let other = Bearer::generate().expect("randomness");
    assert_eq!(bearer.expose().len(), 64);
    assert!(bearer.expose().bytes().all(|b| b.is_ascii_hexdigit()));
    assert!(bearer.matches(bearer.expose().as_bytes()));
    assert!(!bearer.matches(other.expose().as_bytes()));
    assert!(!bearer.matches(&bearer.expose().as_bytes()[..63]));
}

#[test]
fn a_bearer_debugs_as_redacted() {
    let bearer = Bearer::generate().expect("randomness");
    let printed = format!("{bearer:?}");
    assert_eq!(printed, "Bearer(redacted)");
}
