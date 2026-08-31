//! Integration tests for the public crate surface.
//!
//! These tests link against the crate as a downstream consumer would: they can
//! only use what `src/lib.rs` exposes. Treat them as the regression suite for
//! the crate's public contract — if a change breaks a test here, it is a
//! breaking change for users.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tinyteams_core::chat::{GENERAL_DESK, MAIN_THREAD_ID, is_general_chat, same_conversation};

#[test]
fn conversation_identity_is_available_to_consumers() {
    assert!(is_general_chat(Some(MAIN_THREAD_ID)));
    assert!(is_general_chat(Some(GENERAL_DESK)));
    assert!(!is_general_chat(Some("engineering")));
}

#[test]
fn conversation_folding_is_available_to_consumers() {
    assert!(same_conversation(None, Some(GENERAL_DESK)));
    assert!(!same_conversation(Some("engineering"), Some(GENERAL_DESK)));
}

/// The constants are public because a host has to journal under them and pin
/// its own glossary against them.
#[test]
fn the_general_spellings_are_named_constants() {
    assert_eq!(MAIN_THREAD_ID, "main");
    assert_eq!(GENERAL_DESK, "General");
}
