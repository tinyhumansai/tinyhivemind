//! `ProgramTool` as a native tool: its metadata, its permission class, and
//! that every failure reaches the model as a reported error.
use crate::{EnvSecrets, ExecContext, ProgramTool};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use tinyhivemind_lang::ExecTool;
use tinytools::{PermissionLevel, Tool, ToolSideEffects};

fn program(command: &str, extra: Value) -> ProgramTool {
    let mut declaration = json!({
        "id": "greet",
        "description": "Greet someone",
        "command": [command],
        "args": {"type": "object", "properties": {"name": {"type": "string"}}},
        "argv": ["{name}"],
        "stdin": "none",
    });
    if let (Some(base), Value::Object(fields)) = (declaration.as_object_mut(), extra) {
        base.extend(fields);
    }
    let tool: ExecTool = serde_json::from_value(declaration).expect("a valid declaration");
    let context = ExecContext {
        package_root: PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/program/test/fixtures"),
        hive: "acme".into(),
        seat: "critic".into(),
        ..ExecContext::default()
    };
    ProgramTool::new(tool, Arc::new(context), Arc::new(EnvSecrets))
}

#[test]
fn the_tool_is_described_by_its_declaration() {
    let tool = program("tools/greet.sh", json!({}));
    assert_eq!(tool.name(), "greet");
    assert_eq!(tool.description(), "Greet someone");
    assert_eq!(
        tool.parameters_schema(),
        json!({"type": "object", "properties": {"name": {"type": "string"}}})
    );
}

#[test]
fn a_read_tool_is_classified_read_only() {
    let tool = program("tools/greet.sh", json!({"side_effects": "read"}));
    assert_eq!(tool.permission_level(), PermissionLevel::ReadOnly);
    let policy = tool.policy();
    assert!(policy.classified);
    assert_eq!(
        policy.side_effects,
        ToolSideEffects {
            read_only: true,
            ..ToolSideEffects::default()
        }
    );
}

#[test]
fn a_write_tool_needs_write_permission() {
    let tool = program("tools/greet.sh", json!({"side_effects": "write"}));
    assert_eq!(tool.permission_level(), PermissionLevel::Write);
    assert!(!tool.policy().side_effects.read_only);
    assert!(!tool.policy().side_effects.network);
}

#[test]
fn a_network_tool_needs_write_permission_and_declares_the_network() {
    let tool = program("tools/greet.sh", json!({"side_effects": "network"}));
    assert_eq!(tool.permission_level(), PermissionLevel::Write);
    let policy = tool.policy();
    assert!(policy.classified);
    assert!(policy.side_effects.network);
    assert!(!policy.side_effects.read_only);
}

#[tokio::test]
async fn a_successful_call_returns_stdout_with_a_fresh_call_id_each_time() {
    let tool = program("tools/greet.sh", json!({}));
    let first = tool.execute(json!({"name": "ada"})).await.expect("ran");
    let second = tool.execute(json!({"name": "ada"})).await.expect("ran");
    assert!(!first.is_error);
    assert_eq!(first.output(), "hello ada from greet-1\n");
    assert_eq!(second.output(), "hello ada from greet-2\n");
}

#[tokio::test]
async fn a_failing_program_is_reported_to_the_model_not_raised() {
    let tool = program("tools/fail.sh", json!({}));
    let result = tool.execute(json!({"name": "ada"})).await.expect("reported");
    assert!(result.is_error);
    assert_eq!(result.output(), "greet exited with status 2: boom");
}

#[tokio::test]
async fn a_missing_argument_is_reported_to_the_model_not_raised() {
    let tool = program("tools/greet.sh", json!({}));
    let result = tool.execute(json!({})).await.expect("reported");
    assert!(result.is_error);
    assert_eq!(result.output(), "greet requires the argument name");
}

#[tokio::test]
async fn a_truncated_result_says_so() {
    let tool = program("tools/flood.sh", json!({"argv": []}));
    let result = tool.execute(json!({})).await.expect("ran");
    let text = result.output();
    assert!(text.starts_with("aaaa"));
    assert!(text.ends_with("\n[output truncated at 1 MiB]"), "{}", &text[text.len() - 60..]);
}
