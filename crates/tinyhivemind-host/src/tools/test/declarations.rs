//! One vocabulary renders the schemas and validates the arguments.
use crate::{
    ArgumentError, Error,
    tools::{Kind, ToolCall},
};
use serde_json::json;

#[test]
fn every_tool_has_a_distinct_name_that_names_it() {
    assert_eq!(Kind::all(false).len(), 9);
    assert_eq!(Kind::all(true).len(), 13);
    for kind in Kind::all(true) {
        assert!(kind.name().starts_with("hivemind_"));
        assert_ne!(kind.description(), "");
        assert_eq!(Kind::from_name(kind.name()), Some(kind));
    }
    assert_eq!(Kind::from_name("read"), None);
}

#[test]
fn every_tool_schema_is_plain_object_for_strict_providers() {
    const FORBIDDEN: [&str; 6] = ["oneOf", "anyOf", "allOf", "enum", "const", "not"];
    for kind in Kind::all(true) {
        let schema = kind.schema();
        assert_eq!(schema["type"], "object", "{} type", kind.name());
        assert_eq!(schema["additionalProperties"], false, "{}", kind.name());
        assert!(
            schema["properties"].is_object(),
            "{} properties",
            kind.name()
        );
        assert!(schema["required"].is_array(), "{} required", kind.name());
        for key in FORBIDDEN {
            assert!(
                schema.get(key).is_none(),
                "{} has top-level {key}",
                kind.name()
            );
        }
    }
}

#[test]
fn no_tool_declares_or_accepts_a_caller() {
    for kind in Kind::all(true) {
        for caller in ["sender", "actor", "session"] {
            assert!(kind.schema()["properties"].get(caller).is_none());
            assert_eq!(
                kind.validate(&json!({caller: "victim"})),
                Err(ArgumentError::Unknown(caller.into()))
            );
        }
    }
}

#[test]
fn read_schema_and_validator_require_exactly_one_destination() {
    let schema = Kind::Read.schema();
    assert_eq!(schema["required"], json!([]));
    for field in ["hive_id", "agent_id", "thread"] {
        let description = &schema["properties"][field]["description"];
        assert!(description.as_str().is_some_and(|text| {
            text.contains("hive_id") || text.contains("agent_id")
        }));
    }
    for (args, refusal) in [
        (json!({}), ArgumentError::OneDestination),
        (
            json!({"hive_id":"h","agent_id":"b"}),
            ArgumentError::OneDestination,
        ),
        (
            json!({"agent_id":"b","thread":1}),
            ArgumentError::ThreadOnDirectRead,
        ),
    ] {
        assert_eq!(Kind::Read.validate(&args), Err(refusal));
    }
    for args in [
        json!({"agent_id":"b","after":0}),
        json!({"hive_id":"h","thread":1}),
    ] {
        assert_eq!(Kind::Read.validate(&args), Ok(()));
    }
}

#[test]
fn a_call_exists_only_with_valid_arguments() {
    let refused = |kind, args| ToolCall::new("a", None, kind, args).err();
    assert!(matches!(
        refused(Kind::ListAgents, json!([])),
        Some(Error::Arguments(ArgumentError::NotAnObject))
    ));
    assert!(matches!(
        refused(Kind::SendAgent, json!({"agent_id":"a"})),
        Some(Error::Arguments(ArgumentError::Missing("message_id")))
    ));
    assert!(matches!(
        refused(Kind::Read, json!({"hive_id":"h","thread":-1})),
        Some(Error::Arguments(ArgumentError::Invalid {
            name: "thread",
            expected: "integer"
        }))
    ));
    assert!(matches!(
        refused(
            Kind::Ask,
            json!({"episode_id":"e","body":"b","agents":[1]})
        ),
        Some(Error::Arguments(ArgumentError::Invalid {
            name: "agents",
            expected: "array"
        }))
    ));
    assert!(matches!(
        refused(Kind::CreateAgent, json!({"template":"t","config":"no"})),
        Some(Error::Arguments(ArgumentError::Invalid {
            name: "config",
            expected: "object"
        }))
    ));
    assert!(refused(Kind::ListAgents, json!({})).is_none());
}
