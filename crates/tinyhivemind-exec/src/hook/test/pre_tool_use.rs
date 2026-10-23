//! `pre_tool_use`: refusals, matchers, order and failing hooks.
use super::support::{hook, recorded, runner};
use crate::Verdict;
use serde_json::json;
use std::time::{Duration, Instant};

#[tokio::test]
async fn a_deny_verdict_refuses_the_call_with_its_message() {
    let dir = tempfile::tempdir().expect("tempdir");
    let hooks = runner(vec![hook("guard", "pre_tool_use", "deny", json!({}))], dir.path());
    let verdict = hooks.pre_tool_use("git_push", &json!({"branch": "main"})).await;
    assert_eq!(verdict, Verdict::Deny("refused: guard".into()));
}

#[tokio::test]
async fn a_deny_without_a_message_names_the_hook() {
    let dir = tempfile::tempdir().expect("tempdir");
    let hooks = runner(
        vec![hook("guard", "pre_tool_use", "deny_silently", json!({}))],
        dir.path(),
    );
    let verdict = hooks.pre_tool_use("git_push", &json!({})).await;
    assert_eq!(verdict, Verdict::Deny("refused by hook guard".into()));
}

#[tokio::test]
async fn empty_or_unrelated_stdout_allows_the_call() {
    let dir = tempfile::tempdir().expect("tempdir");
    let hooks = runner(
        vec![
            hook("quiet", "pre_tool_use", "allow", json!({})),
            hook("chatty", "pre_tool_use", "chatter", json!({})),
        ],
        dir.path(),
    );
    assert_eq!(hooks.pre_tool_use("read_file", &json!({})).await, Verdict::Allow);
}

#[tokio::test]
async fn the_hook_receives_the_pre_tool_use_event_object() {
    let dir = tempfile::tempdir().expect("tempdir");
    let hooks = runner(vec![hook("audit", "pre_tool_use", "record", json!({}))], dir.path());
    let verdict = hooks.pre_tool_use("git_push", &json!({"branch": "main"})).await;
    assert_eq!(verdict, Verdict::Allow);
    assert_eq!(
        recorded(dir.path()),
        [json!({
            "event": "pre_tool_use",
            "tool": "git_push",
            "arguments": {"branch": "main"},
            "seat": "critic",
            "hive": "acme",
        })]
    );
}

#[tokio::test]
async fn a_matcher_limits_the_hook_to_the_tools_it_names() {
    let dir = tempfile::tempdir().expect("tempdir");
    let hooks = runner(
        vec![hook("guard", "pre_tool_use", "deny", json!({"matcher": "git_*"}))],
        dir.path(),
    );
    assert!(matches!(hooks.pre_tool_use("git_push", &json!({})).await, Verdict::Deny(_)));
    assert_eq!(hooks.pre_tool_use("read_file", &json!({})).await, Verdict::Allow);
}

#[tokio::test]
async fn hooks_run_in_order_and_the_first_deny_wins() {
    let dir = tempfile::tempdir().expect("tempdir");
    let hooks = runner(
        vec![
            hook("audit", "pre_tool_use", "record", json!({})),
            hook("first", "pre_tool_use", "deny", json!({})),
            hook("second", "pre_tool_use", "deny", json!({})),
            hook("late_audit", "pre_tool_use", "record", json!({})),
        ],
        dir.path(),
    );
    let verdict = hooks.pre_tool_use("git_push", &json!({})).await;
    assert_eq!(verdict, Verdict::Deny("refused: first".into()));
    assert_eq!(recorded(dir.path()).len(), 1, "no hook runs after a refusal");
}

#[tokio::test]
async fn a_failing_hook_is_ignored_unless_it_fails_closed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let open = runner(vec![hook("policy", "pre_tool_use", "fail", json!({}))], dir.path());
    assert_eq!(open.pre_tool_use("git_push", &json!({})).await, Verdict::Allow);
    let closed = runner(
        vec![hook("policy", "pre_tool_use", "fail", json!({"fail_closed": true}))],
        dir.path(),
    );
    assert_eq!(
        closed.pre_tool_use("git_push", &json!({})).await,
        Verdict::Deny(
            "hook policy failed: fail exited with status 1: policy store unreachable".into()
        )
    );
}

#[tokio::test]
async fn the_hook_timeout_overrides_the_tool_timeout() {
    let dir = tempfile::tempdir().expect("tempdir");
    let hooks = runner(
        vec![hook(
            "policy",
            "pre_tool_use",
            "slow",
            json!({"fail_closed": true, "timeout_ms": 200}),
        )],
        dir.path(),
    );
    let started = Instant::now();
    let verdict = hooks.pre_tool_use("git_push", &json!({})).await;
    assert!(started.elapsed() < Duration::from_secs(2), "{:?}", started.elapsed());
    assert_eq!(
        verdict,
        Verdict::Deny("hook policy failed: slow timed out after 200 ms".into())
    );
}
