//! Unit tests for conversation identity.
//!
//! The four-spelling fold is the load-bearing rule here, so every spelling is
//! asserted individually rather than through a loop that could pass while
//! silently testing one case four times.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{GENERAL_DESK, MAIN_THREAD_ID, is_general_chat, same_conversation};

/// Every spelling of the default desk, as a fixture the tests below share.
const GENERAL_SPELLINGS: [Option<&str>; 4] = [None, Some(""), Some("main"), Some("General")];

#[test]
fn an_unaddressed_chat_is_general() {
    assert!(is_general_chat(None));
}

#[test]
fn an_empty_chat_id_is_general() {
    assert!(is_general_chat(Some("")));
}

#[test]
fn the_main_thread_id_is_general() {
    assert!(is_general_chat(Some(MAIN_THREAD_ID)));
}

#[test]
fn the_general_desk_id_is_general() {
    assert!(is_general_chat(Some(GENERAL_DESK)));
}

#[test]
fn a_named_desk_is_not_general() {
    assert!(!is_general_chat(Some("engineering")));
}

/// A desk whose id merely *contains* a General spelling is a different desk.
/// The comparison is whole-string, not a prefix or substring test.
#[test]
fn a_desk_whose_id_contains_a_general_spelling_is_not_general() {
    assert!(!is_general_chat(Some("maintenance")));
    assert!(!is_general_chat(Some("general-counsel")));
    assert!(!is_general_chat(Some("the main line")));
}

/// The fold is ASCII-case-insensitive on both spellings. A console that
/// capitalizes its default thread must not open a second transcript.
#[test]
fn general_spellings_fold_regardless_of_case() {
    assert!(is_general_chat(Some("MAIN")));
    assert!(is_general_chat(Some("Main")));
    assert!(is_general_chat(Some("general")));
    assert!(is_general_chat(Some("GENERAL")));
}

#[test]
fn every_general_spelling_names_one_conversation() {
    for a in GENERAL_SPELLINGS {
        for b in GENERAL_SPELLINGS {
            assert!(
                same_conversation(a, b),
                "{a:?} and {b:?} should be the same conversation",
            );
        }
    }
}

#[test]
fn a_named_desk_is_the_same_conversation_as_itself() {
    assert!(same_conversation(Some("engineering"), Some("engineering")));
}

/// The folding is a fact about one desk, not a licence to compare every desk
/// case-insensitively: two desks differing only in case are two desks.
#[test]
fn a_named_desk_compares_verbatim_including_case() {
    assert!(!same_conversation(Some("engineering"), Some("Engineering")));
}

#[test]
fn two_different_named_desks_are_different_conversations() {
    assert!(!same_conversation(Some("engineering"), Some("design")));
}

/// The asymmetric case, and the one worth stating out loud: a General spelling
/// on either side must not swallow a named desk on the other.
#[test]
fn general_is_not_the_same_conversation_as_a_named_desk() {
    for spelling in GENERAL_SPELLINGS {
        assert!(
            !same_conversation(spelling, Some("engineering")),
            "{spelling:?} should not be engineering's conversation",
        );
        assert!(
            !same_conversation(Some("engineering"), spelling),
            "engineering should not be {spelling:?}'s conversation",
        );
    }
}
