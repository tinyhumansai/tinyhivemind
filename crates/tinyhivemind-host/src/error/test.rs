//! The text a model or an operator reads is part of the contract.
use super::{ArgumentError, Error};

#[test]
fn argument_refusals_keep_the_text_the_model_reads() {
    for (error, text) in [
        (ArgumentError::NotAnObject, "arguments must be an object"),
        (
            ArgumentError::Unknown("sender".into()),
            "unknown argument sender",
        ),
        (ArgumentError::Missing("body"), "missing argument body"),
        (
            ArgumentError::Invalid {
                name: "after",
                expected: "integer",
            },
            "invalid after: expected integer",
        ),
        (
            ArgumentError::OneDestination,
            "supply exactly one of hive_id or agent_id",
        ),
        (
            ArgumentError::ThreadOnDirectRead,
            "thread is only valid for hive reads",
        ),
    ] {
        assert_eq!(Error::from(error).to_string(), text);
    }
}

#[test]
fn policy_refusals_and_host_failures_read_as_written() {
    assert_eq!(
        Error::SendDenied("no".into()).to_string(),
        "send denied: no"
    );
    assert_eq!(
        Error::Unauthorized("actor".into()).to_string(),
        "management denied: actor"
    );
    assert_eq!(
        Error::ManagementDisabled.to_string(),
        "management is disabled"
    );
    let own: Box<dyn std::error::Error + Send + Sync> = "the host's own words".into();
    assert_eq!(Error::Host(own).to_string(), "the host's own words");
}
