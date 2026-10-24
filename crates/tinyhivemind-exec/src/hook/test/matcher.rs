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
