//! What the plan's parser refuses: literal secrets, wherever they hide, and
//! fields nobody reads.
use super::*;
use serde_json::{Value, json};

fn parse(value: Value) -> Result<HiveConfig> {
    HiveConfig::from_value(value)
}

/// The field a refusal names, which never repeats the value.
fn refused_field(value: Value) -> Option<String> {
    let error = parse(value).err()?;
    assert!(!format!("{error:?} {error}").contains("hunter2"));
    match error {
        ConfigError::InlineSecret { field } => Some(field),
        _ => None,
    }
}

#[test]
fn a_literal_where_a_secret_reference_belongs_is_refused_by_its_field() {
    for (value, field) in [
        (
            json!({"runtime": {"provider": {"credential": "hunter2"}}}),
            "config.runtime.provider.credential",
        ),
        (
            json!({"runtime": {"provider": {"credential": {"env": " "}}}}),
            "config.runtime.provider.credential",
        ),
        (
            json!({"runtime": {"provider": {"credential": {"env": "A", "store": "b"}}}}),
            "config.runtime.provider.credential",
        ),
        (
            json!({"mcp": [{"env": {"TOKEN": "hunter2"}}]}),
            "config.mcp.env",
        ),
        (
            json!({"mcp": [{"headers": {"authorization": {"plain": "hunter2"}}}]}),
            "config.mcp.headers",
        ),
    ] {
        assert_eq!(
            refused_field(value.clone()).as_deref(),
            Some(field),
            "{value}"
        );
    }
}

#[test]
fn an_endpoint_may_not_carry_a_credential() {
    for endpoint in [
        "https://user:hunter2@example.invalid/v1",
        "https://example.invalid/v1?api_key=hunter2",
        "https://example.invalid/v1?page=1&Access_Token=hunter2",
    ] {
        let value = json!({"runtime": {"provider": {"endpoint": endpoint}}});
        let field = refused_field(value);
        assert_eq!(field.as_deref(), Some("config.runtime.provider.endpoint"));
    }
    for endpoint in [
        "https://example.invalid/v1@2",
        "http://127.0.0.1:1?page=1&flag",
    ] {
        let value = json!({"runtime": {"provider": {"endpoint": endpoint}}});
        assert_eq!(refused_field(value), None, "{endpoint}");
    }
}

#[test]
fn a_secret_reference_is_accepted() {
    for credential in [json!({"env": "NAME"}), json!({"store": "key"}), json!(null)] {
        let value = json!({"runtime": {"provider": {"credential": credential}}});
        assert_eq!(refused_field(value), None);
    }
}

#[test]
fn malformed_json_and_fields_nobody_reads_are_refused() {
    for text in [
        "{",
        r#"{"nobody": 1}"#,
        r#"{"profiles": [{"id": "a", "nobody": 1}]}"#,
    ] {
        let error = HiveConfig::from_json(text).err();
        assert!(
            matches!(error, Some(ConfigError::Json)),
            "{text}: {error:?}"
        );
    }
}

#[test]
fn a_policy_that_ignores_unknown_fields_by_itself_is_still_held_to_its_own() {
    // The native policies would drop this one in silence.
    let value = json!({"hives": [{"id": "desk", "members": [],
        "coordinator": {"conduct_policy": {"nobody": 1}}}]});
    assert!(matches!(parse(value), Err(ConfigError::Json)));
}
