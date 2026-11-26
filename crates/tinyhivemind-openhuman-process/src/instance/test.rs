//! Start-up verification and tool discovery, against the fake surface.
use super::verify::{discover, verify};
use super::{Layout, environment, turn_secs};
use crate::error::Error;
use crate::fixture::{Fixture, PID, SERVER, hashed_tools, served};
use std::collections::BTreeSet;
use std::time::Duration;
use tinyhivemind_host::harness::Mismatch;

const WARM: Duration = Duration::from_secs(5);

async fn verified(
    fixture: &Fixture,
    pid: u32,
) -> Result<tinyhivemind_host::harness::Started, Error> {
    let tools = hashed_tools();
    verify(
        &fixture.client,
        pid,
        "0.64.15",
        &fixture.written,
        &tools,
        WARM,
    )
    .await
}

#[tokio::test]
async fn the_instance_that_was_written_is_verified_after_its_warm_up() {
    let fixture = Fixture::with(&[("warm", "3")]).await;
    let started = verified(&fixture, PID).await;
    let started = started.unwrap_or_else(|error| unreachable!("verified: {error}"));
    assert_eq!(started.pid, PID);
    assert_eq!(
        started.tools,
        hashed_tools().into_iter().collect::<BTreeSet<_>>()
    );
    assert!(started.settings.contains("agent_registry.entries"));
    assert!(started.settings.contains("api_url"));
}

#[tokio::test]
async fn another_process_answering_is_refused() {
    let fixture = Fixture::new().await;
    let refused = verified(&fixture, PID + 1).await;
    assert!(
        matches!(refused, Err(Error::Mismatch(Mismatch::Pid { .. }))),
        "{refused:?}"
    );
}

#[tokio::test]
async fn another_version_is_refused() {
    let fixture = Fixture::with(&[("version", "0.64.14")]).await;
    let refused = verified(&fixture, PID).await;
    assert!(
        matches!(refused, Err(Error::Mismatch(Mismatch::Version { .. }))),
        "{refused:?}"
    );
}

#[tokio::test]
async fn a_config_that_fell_back_to_defaults_is_refused() {
    let fixture = Fixture::with(&[("misconfigured", "")]).await;
    let refused = verified(&fixture, PID).await;
    assert!(
        matches!(refused, Err(Error::Mismatch(Mismatch::Settings { .. }))),
        "{refused:?}"
    );
}

#[tokio::test]
async fn a_tool_the_model_would_see_besides_the_hive_tools_is_refused() {
    let fixture = Fixture::with(&[("tools", "extra")]).await;
    let refused = verified(&fixture, PID).await;
    assert!(matches!(refused, Err(Error::Mismatch(_))), "{refused:?}");
}

#[tokio::test]
async fn a_hive_tool_the_model_would_not_see_is_refused() {
    let fixture = Fixture::with(&[("tools", "missing"), ("warm", "0")]).await;
    let tools = hashed_tools();
    let refused = verify(
        &fixture.client,
        PID,
        "0.64.15",
        &fixture.written,
        &tools,
        Duration::ZERO,
    )
    .await;
    let Err(Error::Mismatch(Mismatch::Tools {
        missing,
        unexpected,
    })) = refused
    else {
        unreachable!("refused for its tools: {refused:?}")
    };
    assert_eq!(missing, [tools[1].clone()]);
    assert_eq!(unexpected, Vec::<String>::new());
}

#[tokio::test]
async fn discovery_waits_out_the_warm_up_and_reads_the_hashed_names() {
    let fixture = Fixture::with(&[("warm", "4")]).await;
    let wanted: BTreeSet<String> = served().into_iter().collect();
    let found = discover(&fixture.client, SERVER, &wanted, WARM).await;
    let mut expected = hashed_tools();
    expected.sort();
    assert_eq!(found.ok(), Some(expected));
}

#[tokio::test]
async fn a_tool_never_listed_fails_discovery_by_name() {
    let fixture = Fixture::with(&[("tools", "missing")]).await;
    let wanted: BTreeSet<String> = served().into_iter().collect();
    let found = discover(&fixture.client, SERVER, &wanted, Duration::from_millis(300)).await;
    let Err(Error::ToolsNotListed { missing, .. }) = found else {
        unreachable!("not listed: {found:?}")
    };
    assert_eq!(missing, ["hivemind_ask"]);
}

#[tokio::test]
async fn a_tool_whose_name_another_tools_name_starts_with_is_told_apart() {
    let fixture = Fixture::with(&[("served", r#"["post", "post_x"]"#), ("warm", "0")]).await;
    let wanted: BTreeSet<String> = ["post".to_owned()].into_iter().collect();
    let found = discover(&fixture.client, SERVER, &wanted, WARM).await;
    assert_eq!(found.ok(), Some(vec![crate::fake::hashed(SERVER, "post")]));
}

#[test]
fn the_environment_is_exactly_the_recipe_and_never_turns_the_sandbox_off() {
    let layout = Layout::new(std::path::Path::new("/seat/state"));
    let environment = environment(&layout, "token", "key");
    let names: Vec<&str> = environment.iter().map(|(name, _)| *name).collect();
    assert_eq!(
        names,
        [
            "PATH",
            "HOME",
            "LANG",
            "OPENHUMAN_WORKSPACE",
            "OPENHUMAN_ACTION_DIR",
            "OPENHUMAN_TOOL_DISPATCHER",
            "OPENHUMAN_COMPOSIO_MODE",
            "OPENHUMAN_BACKEND_API_KEY",
            "OPENHUMAN_KEYRING_BACKEND",
            "OPENHUMAN_KEYRING_MASTER_KEY",
            "OPENHUMAN_CORE_TOKEN",
            "RUST_LOG",
        ]
    );
    let value = |name: &str| {
        environment
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| value.as_str())
    };
    assert_eq!(value("OPENHUMAN_WORKSPACE"), Some("/seat/state/ws"));
    assert_eq!(value("OPENHUMAN_KEYRING_BACKEND"), Some("encrypted_file"));
    assert_eq!(value("OPENHUMAN_BACKEND_API_KEY"), Some("dummy"));
    assert!(value("OPENHUMAN_SANDBOX").is_none());
}

#[test]
fn the_binary_lets_a_turn_run_past_the_seats_own_limit() {
    assert_eq!(turn_secs(None), 600);
    assert_eq!(turn_secs(Some(Duration::from_secs(5))), 600);
    assert_eq!(turn_secs(Some(Duration::from_secs(3600))), 3660);
}

#[test]
fn compaction_puts_the_recovery_tool_before_the_model_whatever_the_allowlist() {
    let available = ["juice_retrieve".to_owned(), "mcp_s_post_1".to_owned()];
    let agent =
        serde_json::json!({"id": crate::config::AGENT_ID, "tool_allowlist": ["mcp_s_post_1"]});
    let on = serde_json::json!({"agent_registry": {"entries": [agent.clone()]}});
    assert_eq!(
        super::verify::visible(&on, &available),
        ["mcp_s_post_1", "juice_retrieve"]
    );
    let off = serde_json::json!({"agent_registry": {"entries": [agent]}, "context": {"compaction_enabled": false}});
    assert_eq!(super::verify::visible(&off, &available), ["mcp_s_post_1"]);
}

#[test]
fn the_written_config_turns_compaction_off() {
    let settings = Fixture::written_for_test().settings();
    let compaction = settings
        .iter()
        .find(|(name, _)| name == "context.compaction_enabled");
    assert_eq!(
        compaction.map(|(_, value)| value.clone()),
        Some(serde_json::json!(false))
    );
}

/// What `resolved_scope` in 0.64.15's `definition_registry.rs` adds to an
/// allowlist, each of which the start check must see.
#[test]
fn every_source_the_binary_adds_tools_from_is_counted() {
    use serde_json::json;
    let available: Vec<String> = ["mcp_s_post_1", "tool_search", "juice_retrieve", "juice_find", "juice_extract", "juice_summarize", "shell"]
        .map(String::from)
        .to_vec();
    let entry = |extra: serde_json::Value| {
        let mut entry = json!({"id": crate::config::AGENT_ID, "tool_allowlist": ["mcp_s_post_1"]});
        if let (Some(entry), Some(extra)) = (entry.as_object_mut(), extra.as_object()) {
            entry.extend(extra.clone());
        }
        entry
    };
    let config = |entry: serde_json::Value, rest: serde_json::Value| {
        let mut config = json!({
            "agent_registry": {"entries": [entry]},
            "context": {"compaction_enabled": false},
            "agent": {"chat_agent_id": crate::config::AGENT_ID},
        });
        if let (Some(config), Some(rest)) = (config.as_object_mut(), rest.as_object()) {
            for (key, value) in rest {
                config.insert(key.clone(), value.clone());
            }
        }
        config
    };
    let seen = |config: &serde_json::Value| super::verify::visible(config, &available);
    assert_eq!(seen(&config(entry(json!({})), json!({}))), ["mcp_s_post_1"]);
    // Deferred tools, when the allowlist names the tool search.
    let search = entry(json!({"tool_allowlist": ["mcp_s_post_1", "tool_search"]}));
    assert!(seen(&config(search, json!({}))).iter().any(|tool| tool.contains("deferred")));
    // The REPL tools, while results are stored behind a handle.
    let repl = json!({
        "context": {"compaction_enabled": true},
        "tokenjuice": {"router_enabled": true, "ccr_enabled": true, "repl_handle_enabled": true},
    });
    let repl = seen(&config(entry(json!({})), repl));
    for tool in ["juice_retrieve", "juice_find", "juice_extract", "juice_summarize"] {
        assert!(repl.contains(&tool.to_owned()), "{tool} in {repl:?}");
    }
    // The summary stage, which only the orchestrator gets.
    let orchestrator = json!({"id": "orchestrator", "tool_allowlist": ["mcp_s_post_1"]});
    let summary = json!({
        "agent": {"chat_agent_id": "orchestrator"},
        "context": {"compaction_enabled": false, "summarizer_payload_threshold_tokens": 100},
    });
    let summary = seen(&config(orchestrator, summary));
    assert!(summary.contains(&"juice_retrieve".to_owned()), "{summary:?}");
    // `extra_tools`, and a session's delegation tools for an agent with subagents.
    let extra = seen(&config(entry(json!({"extra_tools": ["shell"]})), json!({})));
    assert!(extra.contains(&"shell".to_owned()), "{extra:?}");
    let delegating = seen(&config(entry(json!({"subagents": ["helper"]})), json!({})));
    assert!(delegating.iter().any(|tool| tool.contains("delegation")), "{delegating:?}");
}
