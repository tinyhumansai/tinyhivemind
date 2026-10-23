//! The scrubbed environment and the working directory.
use super::support::{FixedSecrets, context, fixtures, tool};
use crate::{Error, run};
use serde_json::json;
use std::collections::BTreeMap;

/// Variables a POSIX shell sets on its own, which are not inherited.
const SHELL_OWN: [&str; 3] = ["PWD", "SHLVL", "_"];

async fn environment(context: &crate::ExecContext) -> BTreeMap<String, String> {
    let tool = tool(
        &["tools/env.sh"],
        json!({"env": {"TOKEN": {"env": "GH_TOKEN"}}, "stdin": "none"}),
    );
    let output = run(&tool, context, &FixedSecrets, &json!({}), "call-7")
        .await
        .expect("ran");
    output
        .text
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(name, value)| (name.to_owned(), value.to_owned()))
        .collect()
}

#[tokio::test]
async fn the_environment_holds_only_the_convention() {
    let mut context = context();
    context.features = BTreeMap::from([("beta".into(), true), ("old_ui".into(), false)]);
    let env = environment(&context).await;
    let root = std::path::absolute(fixtures()).expect("absolute");
    let expected = [
        ("TOKEN", "value-of-GH_TOKEN"),
        ("THM_HIVE", "acme"),
        ("THM_SEAT", "critic"),
        ("THM_TOOL", "probe"),
        ("THM_CALL_ID", "call-7"),
        ("THM_PACKAGE_ROOT", root.to_str().expect("utf-8")),
        ("THM_FEATURE_BETA", "1"),
        ("THM_FEATURE_OLD_UI", "0"),
    ];
    for (name, value) in expected {
        assert_eq!(env.get(name).map(String::as_str), Some(value), "{name}");
    }
    let inherited = ["PATH", "HOME", "LANG"];
    for name in env.keys() {
        let declared = expected.iter().any(|(n, _)| n == name);
        assert!(
            declared || inherited.contains(&name.as_str()) || SHELL_OWN.contains(&name.as_str()),
            "{name} leaked into the environment"
        );
    }
    for name in inherited {
        assert_eq!(env.get(name).cloned(), std::env::var(name).ok(), "{name}");
    }
    assert!(
        std::env::var_os("CARGO_PKG_NAME").is_some(),
        "cargo sets it for tests"
    );
    assert!(!env.contains_key("CARGO_PKG_NAME"));
}

#[tokio::test]
async fn an_unresolved_secret_fails_the_call() {
    let tool = tool(
        &["tools/env.sh"],
        json!({"env": {"TOKEN": {"store": "gh"}}}),
    );
    let error = run(&tool, &context(), &FixedSecrets, &json!({}), "c1")
        .await
        .expect_err("unresolved");
    assert!(matches!(error, Error::UnresolvedSecret { .. }));
}

async fn working_directory(context: &crate::ExecContext, cwd: &str) -> std::path::PathBuf {
    let tool = tool(&["tools/pwd.sh"], json!({"cwd": cwd, "stdin": "none"}));
    let output = run(&tool, context, &FixedSecrets, &json!({}), "c1")
        .await
        .expect("ran");
    output.text.trim_end().into()
}

#[tokio::test]
async fn cwd_package_runs_in_the_package_root() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let mut context = context();
    context.workspace = Some(workspace.path().to_path_buf());
    let root = fixtures().canonicalize().expect("canonical");
    assert_eq!(working_directory(&context, "package").await, root);
}

#[tokio::test]
async fn cwd_workspace_runs_in_the_seat_workspace() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let mut context = context();
    context.workspace = Some(workspace.path().to_path_buf());
    let expected = workspace.path().canonicalize().expect("canonical");
    assert_eq!(working_directory(&context, "workspace").await, expected);
}

#[tokio::test]
async fn cwd_workspace_without_a_workspace_runs_in_the_package_root() {
    let root = fixtures().canonicalize().expect("canonical");
    assert_eq!(working_directory(&context(), "workspace").await, root);
}
