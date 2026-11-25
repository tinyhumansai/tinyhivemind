//! `*` wildcards over tool names.
use crate::hook::matches;

#[test]
fn no_matcher_matches_every_tool() {
    assert!(matches(None, "anything"));
}

#[test]
fn a_literal_matcher_matches_only_that_tool() {
    assert!(matches(Some("git_push"), "git_push"));
    assert!(!matches(Some("git_push"), "git_pushed"));
    assert!(!matches(Some("git_push"), "a_git_push"));
}

#[test]
fn a_star_matches_any_run_of_characters() {
    assert!(matches(Some("*"), ""));
    assert!(matches(Some("*"), "read_file"));
    assert!(matches(Some("git_*"), "git_"));
    assert!(matches(Some("git_*"), "git_push"));
    assert!(!matches(Some("git_*"), "gh_push"));
    assert!(matches(Some("*_file"), "read_file"));
    assert!(matches(Some("a*b*c"), "a__b__c"));
    assert!(matches(Some("a*b*c"), "abc"));
    assert!(!matches(Some("a*b*c"), "a__c__b"));
    assert!(matches(Some("**"), "x"));
}

#[test]
fn a_runner_says_whether_a_tool_hook_names_a_tool() {
    use super::support::{hook, runner};
    use serde_json::json;
    let dir = std::env::temp_dir();
    let hooks = runner(
        vec![
            hook("guard", "pre_tool_use", "allow", json!({"matcher": "git_*"})),
            hook("audit", "post_tool_use", "record", json!({"matcher": "deploy"})),
            hook("end", "stop", "record", json!({})),
        ],
        &dir,
    );
    assert!(hooks.matches_tool("git_push"));
    assert!(hooks.matches_tool("deploy"));
    // A stop hook is bound to no tool.
    assert!(!hooks.matches_tool("read_file"));
}
