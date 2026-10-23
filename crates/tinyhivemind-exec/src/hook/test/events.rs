//! `post_tool_use` and `stop`, and which hooks an event runs.
use super::support::{hook, recorded, runner};
use serde_json::json;

#[tokio::test]
async fn post_tool_use_runs_its_hooks_with_the_result() {
    let dir = tempfile::tempdir().expect("tempdir");
    let hooks = runner(
        vec![
            hook("guard", "pre_tool_use", "record", json!({})),
            hook("audit", "post_tool_use", "record", json!({"matcher": "git_*"})),
            hook("broken", "post_tool_use", "fail", json!({"fail_closed": true})),
        ],
        dir.path(),
    );
    hooks.post_tool_use("git_push", &json!({"branch": "main"}), "pushed").await;
    hooks.post_tool_use("read_file", &json!({}), "contents").await;
    assert_eq!(
        recorded(dir.path()),
        [json!({
            "event": "post_tool_use",
            "tool": "git_push",
            "arguments": {"branch": "main"},
            "result": "pushed",
            "seat": "critic",
            "hive": "acme",
        })]
    );
}

#[tokio::test]
async fn stop_returns_the_first_followup() {
    let dir = tempfile::tempdir().expect("tempdir");
    let hooks = runner(
        vec![
            hook("audit", "stop", "record", json!({})),
            hook("broken", "stop", "fail", json!({"fail_closed": true})),
            hook("nag", "stop", "followup", json!({})),
        ],
        dir.path(),
    );
    let followup = hooks.stop("all done").await;
    assert_eq!(followup.as_deref(), Some("run the tests before stopping"));
    assert_eq!(
        recorded(dir.path()),
        [json!({"event": "stop", "reply": "all done", "seat": "critic", "hive": "acme"})]
    );
}

#[tokio::test]
async fn stop_without_a_followup_ends_the_turn() {
    let dir = tempfile::tempdir().expect("tempdir");
    let hooks = runner(
        vec![
            hook("quiet", "stop", "allow", json!({})),
            hook("guard", "pre_tool_use", "followup", json!({})),
        ],
        dir.path(),
    );
    assert_eq!(hooks.stop("all done").await, None);
}

#[test]
fn a_runner_without_hooks_is_empty() {
    let dir = tempfile::tempdir().expect("tempdir");
    assert!(runner(Vec::new(), dir.path()).is_empty());
    let hooks = vec![hook("quiet", "stop", "allow", json!({}))];
    assert!(!runner(hooks, dir.path()).is_empty());
}
