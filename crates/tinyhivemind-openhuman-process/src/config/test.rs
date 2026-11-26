//! The generated config: what it says, that it parses back, and how an
//! effective config is held against it.
use super::{AGENT_ID, Input, Written};
use crate::capability::SeatModel;
use serde_json::{Value, json};

fn model() -> SeatModel {
    SeatModel {
        provider: "local".into(),
        endpoint: "http://127.0.0.1:9/v1".into(),
        model: "mock".into(),
        iterations: 8,
    }
}

fn written(prompt: &str, tools: &[String]) -> Written {
    Written::new(&Input {
        model: &model(),
        seat: "scout",
        prompt,
        server: "hivemind",
        url: "http://127.0.0.1:7/mcp",
        bearer: "secret-bearer",
        tools,
        turn_secs: 600,
    })
}

fn tools() -> Vec<String> {
    vec![
        "mcp_hivemind_post_1a2b3c".into(),
        "mcp_hivemind_ask_4d5e6f".into(),
    ]
}

/// The file as `openhuman-core` reads it.
fn parsed(written: &Written) -> Value {
    let text = written.toml();
    let table: ::toml::Table = text
        .parse()
        .unwrap_or_else(|error| unreachable!("the config does not parse: {error}\n{text}"));
    serde_json::to_value(table).unwrap_or_else(|error| unreachable!("{error}"))
}

#[test]
fn the_config_parses_back_to_what_was_built() {
    let written = written("You scout.", &tools());
    assert_eq!(parsed(&written), written.tree);
}

#[test]
fn the_allowlist_and_auto_approve_are_exactly_the_hive_tools() {
    let config = parsed(&written("You scout.", &tools()));
    let agent = &config["agent_registry"]["entries"][0];
    assert_eq!(agent["id"], AGENT_ID);
    assert_eq!(agent["tool_allowlist"], json!(tools()));
    assert!(agent.get("tool_denylist").is_none());
    assert_eq!(config["autonomy"]["auto_approve"], json!(tools()));
    assert_eq!(config["autonomy"]["auto_approve_all"], false);
    assert_eq!(config["agent"]["chat_agent_id"], AGENT_ID);
    let text = written("You scout.", &tools()).toml();
    assert!(!text.contains("\"*\""), "{text}");
}

#[test]
fn the_provider_is_the_seats_alone_and_needs_no_key() {
    let config = parsed(&written("You scout.", &tools()));
    let providers = config["cloud_providers"].as_array().cloned();
    let providers = providers.unwrap_or_default();
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0]["endpoint"], "http://127.0.0.1:9/v1");
    assert_eq!(providers[0]["auth_style"], "none");
    assert_eq!(config["primary_cloud"], providers[0]["id"]);
    for route in [
        "chat_provider",
        "reasoning_provider",
        "agentic_provider",
        "coding_provider",
    ] {
        assert_eq!(config[route], "thm:mock", "{route}");
    }
}

#[test]
fn the_host_server_is_direct_with_the_seats_bearer() {
    let config = parsed(&written("You scout.", &tools()));
    let servers = &config["mcp_client"]["servers"];
    assert_eq!(servers.as_array().map(Vec::len), Some(1));
    assert_eq!(servers[0]["name"], "hivemind");
    assert_eq!(servers[0]["endpoint"], "http://127.0.0.1:7/mcp");
    assert_eq!(servers[0]["expose"], "direct");
    assert_eq!(
        servers[0]["auth"],
        json!({"kind": "bearer_token", "token": "secret-bearer"})
    );
}

#[test]
fn memory_search_hooks_suggestions_and_compression_are_off() {
    let config = parsed(&written("You scout.", &tools()));
    assert_eq!(config["memory"]["engine"], "");
    assert_eq!(config["memory"]["recall"]["enabled"], false);
    assert_eq!(config["memory"]["conversations"]["enabled"], false);
    assert_eq!(config["search"]["enabled"], false);
    assert_eq!(config["hooks"]["enabled"], false);
    assert_eq!(config["web_chat"]["suggestions_enabled"], false);
    for flag in ["router_enabled", "ccr_enabled", "repl_handle_enabled"] {
        assert_eq!(config["tokenjuice"][flag], false, "{flag}");
    }
    assert_eq!(config["observability"]["analytics_enabled"], false);
}

#[test]
fn a_prompt_cannot_break_out_of_its_string() {
    let prompt = "line one\n[hooks]\nenabled = true\n\"\"\" ' \\ \u{1}";
    let config = parsed(&written(prompt, &tools()));
    assert_eq!(
        config["agent_registry"]["entries"][0]["system_prompt"],
        prompt
    );
    assert_eq!(config["hooks"], json!({"enabled": false}));
}

#[test]
fn settings_name_every_leaf_written_and_the_routes_left_unset() {
    let settings = written("You scout.", &tools()).settings();
    let names: Vec<&str> = settings.iter().map(|(name, _)| name.as_str()).collect();
    for name in [
        "primary_cloud",
        "memory.recall.enabled",
        "mcp_client.servers",
        "agent_registry.entries",
        "autonomy.auto_approve",
        "api_key",
        "inference_url",
    ] {
        assert!(names.contains(&name), "{name} in {names:?}");
    }
    let unset = settings.iter().find(|(name, _)| name == "api_url");
    assert_eq!(unset.map(|(_, value)| value), Some(&Value::Null));
}

/// What `config_get` returns for the written file: the same, plus defaults.
fn effective(written: &Written) -> Value {
    let mut config = written.tree.clone();
    config["agent"]["max_history_messages"] = json!(50);
    config["mcp_client"]["servers"][0]["timeout_secs"] = json!(30);
    config["mcp_client"]["servers"][0]["allowed_tools"] = json!([]);
    config["embeddings_provider"] = json!("openhuman");
    for name in ["api_key", "api_url", "inference_url"] {
        config[name] = Value::Null;
    }
    config
}

#[test]
fn an_effective_config_with_defaults_added_is_the_one_written() {
    let written = written("You scout.", &tools());
    let observed = written.observe(&effective(&written));
    assert_eq!(observed, written.settings());
}

#[test]
fn an_effective_config_that_differs_anywhere_written_is_another_one() {
    let written = written("You scout.", &tools());
    let changes: [fn(&mut Value); 5] = [
        |config| config["hooks"]["enabled"] = json!(true),
        |config| config["api_url"] = json!("https://hosted.example"),
        |config| {
            let extra = json!({"name": "other", "endpoint": "http://127.0.0.1:8/mcp"});
            if let Some(servers) = config["mcp_client"]["servers"].as_array_mut() {
                servers.push(extra);
            }
        },
        |config| {
            let tools = &mut config["agent_registry"]["entries"][0]["tool_allowlist"];
            if let Some(tools) = tools.as_array_mut() {
                tools.push(json!("shell"));
            }
        },
        // What a config that did not parse falls back to.
        |config| *config = json!({"schema_version": 13}),
    ];
    for change in changes {
        let mut config = effective(&written);
        change(&mut config);
        assert_ne!(written.observe(&config), written.settings(), "{config}");
    }
}

#[test]
fn the_writer_leaves_out_null_and_writes_other_shapes_inline() {
    let tree = json!({
        "gone": null,
        "empty": [],
        "mixed": [1, {"a": "b"}],
        "table": {"inner": {"deep": true}},
    });
    let text = super::toml::write(&tree);
    assert!(!text.contains("gone"), "{text}");
    let table: ::toml::Table = text
        .parse()
        .unwrap_or_else(|error| unreachable!("{error}\n{text}"));
    let parsed = serde_json::to_value(table).unwrap_or_else(|error| unreachable!("{error}"));
    let expected = json!({
        "empty": [],
        "mixed": [1, {"a": "b"}],
        "table": {"inner": {"deep": true}},
    });
    assert_eq!(parsed, expected);
    assert_eq!(super::toml::write(&json!("not a table")), "");
}
