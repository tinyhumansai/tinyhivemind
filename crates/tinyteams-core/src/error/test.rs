//! Unit tests for the crate-wide error type.

use super::Error;

#[test]
fn errors_are_typed_standard_errors_with_lowercase_unpunctuated_messages() {
    fn assert_standard_error(_: &impl std::error::Error) {}

    let errors = [
        Error::EmptyDeskId,
        Error::EmptyDeskName {
            desk_id: "d".into(),
        },
        Error::DuplicateDeskId {
            desk_id: "d".into(),
        },
        Error::ReservedDeskIdentity {
            identity: "main".into(),
        },
        Error::AmbiguousDesk {
            identity: "Desk".into(),
        },
        Error::UnknownDesk {
            identity: "d".into(),
        },
        Error::UnknownMemberDesk {
            desk_id: "d".into(),
        },
        Error::UnknownOrderDesk {
            desk_id: "d".into(),
        },
        Error::DuplicateDeskOrder {
            desk_id: "d".into(),
        },
        Error::DuplicateOrderMember {
            desk_id: "d".into(),
            agent_id: "a".into(),
        },
        Error::UnknownOrderMember {
            desk_id: "d".into(),
            agent_id: "a".into(),
        },
        Error::IncompleteOrder {
            desk_id: "d".into(),
            missing_agent_id: "a".into(),
        },
    ];

    for error in errors {
        assert_standard_error(&error);
        let message = error.to_string();
        assert!(message.starts_with(|character: char| character.is_ascii_lowercase()));
        assert!(!message.ends_with(['.', '!', '?']));
    }
}
