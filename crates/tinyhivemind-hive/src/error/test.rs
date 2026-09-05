//! Unit tests for the crate-wide error type.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

use std::error::Error as _;

#[test]
fn every_message_is_lowercase_without_trailing_punctuation() {
    let errors = [
        Error::from(tinyhivemind_core::error::Error::EmptyDeskId),
        Error::DuplicateAgentThreshold {
            agent_id: "planner".into(),
        },
        Error::UnknownThresholdMember {
            agent_id: "planner".into(),
            desk_id: "engineering".into(),
        },
        Error::ZeroHalfLife,
        Error::ZeroQuorumThreshold,
        Error::ZeroQuorumWindow,
        Error::ZeroRefutationCap,
        Error::ZeroDirectoryHalfLife,
        Error::ZeroDirectoryWindow,
        Error::ZeroDeferCap,
    ];
    for error in errors {
        let message = error.to_string();
        assert!(!message.is_empty(), "{error:?} has an empty message");
        assert!(
            !message.ends_with(['.', '!', '?']),
            "{message:?} ends with punctuation",
        );
        let first = message.chars().next().expect("a first character");
        assert!(
            !first.is_uppercase(),
            "{message:?} starts with an uppercase letter",
        );
    }
}

#[test]
fn a_core_failure_is_carried_verbatim_and_kept_as_a_source() {
    let error = Error::from(tinyhivemind_core::error::Error::UnknownDesk {
        identity: "design".into(),
    });
    assert_eq!(error.to_string(), "unknown desk `design`");
    assert!(
        error.source().is_some(),
        "the pure-algebra failure must remain reachable as a source",
    );
}

#[test]
fn the_result_alias_carries_the_crate_error() {
    fn failing() -> Result<()> {
        Err(Error::ZeroHalfLife)
    }
    assert!(failing().is_err());
}

#[test]
fn a_malformed_directory_policy_produces_its_own_variant() {
    use crate::directory::{DirectoryPolicy, directory};
    use tinyhivemind::Sequence;

    let zero_half_life = DirectoryPolicy {
        half_life: 0,
        ..DirectoryPolicy::DEFAULT
    };
    assert!(matches!(
        directory(&[], Sequence(1), &zero_half_life, &[]),
        Err(Error::ZeroDirectoryHalfLife),
    ));

    let zero_window = DirectoryPolicy {
        window: 0,
        ..DirectoryPolicy::DEFAULT
    };
    assert!(matches!(
        directory(&[], Sequence(1), &zero_window, &[]),
        Err(Error::ZeroDirectoryWindow),
    ));
}
