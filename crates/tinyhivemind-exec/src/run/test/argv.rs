//! `command ++ argv` with placeholders substituted.
use crate::Error;
use crate::run::command::arguments;
use serde_json::json;
use tinyhivemind_lang::ExecTool;

fn tool(command: &[&str], argv: &[&str]) -> ExecTool {
    serde_json::from_value(json!({
        "id": "probe",
        "description": "probe",
        "command": command,
        "argv": argv,
    }))
    .expect("a valid declaration")
}

#[test]
fn fixed_arguments_precede_substituted_ones() {
    let tool = tool(&["tools/a.sh", "--fixed"], &["--pattern", "{pattern}"]);
    let argv = arguments(&tool, &json!({"pattern": "fn main"})).expect("argv");
    assert_eq!(argv, ["--fixed", "--pattern", "fn main"]);
}

#[test]
fn an_absent_optional_placeholder_is_dropped() {
    let tool = tool(&["tools/a.sh"], &["{pattern}", "{path?}", "--end"]);
    let argv = arguments(&tool, &json!({"pattern": "x"})).expect("argv");
    assert_eq!(argv, ["x", "--end"]);
    let argv = arguments(&tool, &json!({"pattern": "x", "path": null})).expect("argv");
    assert_eq!(argv, ["x", "--end"]);
    let argv = arguments(&tool, &json!({"pattern": "x", "path": "src"})).expect("argv");
    assert_eq!(argv, ["x", "src", "--end"]);
}

#[test]
fn a_non_string_argument_is_substituted_as_compact_json() {
    let tool = tool(&["tools/a.sh"], &["{count}", "{flags}", "{filter?}"]);
    let call = json!({"count": 3, "flags": [true, "a b"], "filter": {"k": 1}});
    let argv = arguments(&tool, &call).expect("argv");
    assert_eq!(argv, ["3", r#"[true,"a b"]"#, r#"{"k":1}"#]);
}

#[test]
fn a_placeholder_must_be_the_whole_element() {
    let tool = tool(&["tools/a.sh"], &["--pattern={pattern}", "{}"]);
    let argv = arguments(&tool, &json!({"pattern": "x"})).expect("argv");
    assert_eq!(argv, ["--pattern={pattern}", "{}"]);
}

#[test]
fn an_absent_required_placeholder_fails_the_call() {
    let tool = tool(&["tools/a.sh"], &["{pattern}"]);
    let error = arguments(&tool, &json!({})).expect_err("missing");
    assert!(
        matches!(&error, Error::MissingArgument { tool, name } if tool == "probe" && name == "pattern")
    );
    assert_eq!(error.to_string(), "probe requires the argument pattern");
    assert!(arguments(&tool, &json!({"pattern": null})).is_err());
}
