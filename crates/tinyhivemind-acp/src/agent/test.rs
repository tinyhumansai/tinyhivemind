//! Descriptors and their templates.
#![allow(clippy::unwrap_used)] // Tests fail loudly on unexpected results.
use super::*;
use serde_json::json;

fn values() -> Values {
    Values {
        values: BTreeMap::from([
            ("state".to_owned(), "/s".to_owned()),
            ("endpoint".to_owned(), "http://127.0.0.1:9".to_owned()),
            ("model".to_owned(), "mock".to_owned()),
        ]),
        allowed_tools: vec!["mcp__hive__hivemind_post".into()],
    }
}

#[test]
fn templates_fill_every_known_name_and_leave_the_rest() {
    let values = values();
    assert_eq!(
        values.fill("{state}/claude at {endpoint} as {model}"),
        "/s/claude at http://127.0.0.1:9 as mock"
    );
    assert_eq!(values.fill("{unknown}"), "{unknown}");
    let meta = json!({"a": ["{state}", 1, null], "allowed": "{allowed_tools}", "keep": "{allowed_tools} x"});
    assert_eq!(
        values.fill_json(&meta),
        json!({"a": ["/s", 1, null], "allowed": ["mcp__hive__hivemind_post"], "keep": "{allowed_tools} x"})
    );
}

#[test]
fn claude_code_turns_its_own_tools_off_and_allows_the_hosts() {
    let agent = Agent::claude_code("/opt/node/bin/node", "/opt/acp/index.js", "0.112.0");
    assert_eq!(agent.kind, HarnessKind::ClaudeCode);
    assert_eq!(agent.args, ["/opt/acp/index.js"]);
    let env: BTreeMap<_, _> = agent.env.iter().cloned().collect();
    assert_eq!(env["ANTHROPIC_BASE_URL"], "{endpoint}");
    assert_eq!(env["ANTHROPIC_API_KEY"], NO_KEY);
    assert_eq!(env["ENABLE_TOOL_SEARCH"], "false");
    assert_eq!(env["HOME"], "{home}");
    let options = &agent.session_meta.as_ref().unwrap()["claudeCode"]["options"];
    assert_eq!(options["tools"], json!([]));
    assert_eq!(options["settingSources"], json!([]));
    assert_eq!(options["allowedTools"], "{allowed_tools}");
    assert_eq!(agent.mode, None);
}

#[test]
fn codex_writes_its_config_and_runs_read_only() {
    let agent = Agent::codex(
        "/opt/node/bin/node",
        "/opt/acp/index.js",
        "2.12.1",
        "/opt/catalog.json",
    );
    assert_eq!(agent.kind, HarnessKind::Codex);
    assert_eq!(agent.mode.as_deref(), Some("read-only"));
    assert_eq!(agent.vars["catalog"], "/opt/catalog.json");
    let (path, config) = &agent.files[0];
    assert_eq!(path, Path::new("codex/config.toml"));
    let mut values = values();
    values.values.extend(agent.vars.clone());
    let config = values.fill(config);
    assert!(config.contains("base_url = \"http://127.0.0.1:9\""));
    assert!(config.contains("model = \"mock\""));
    assert!(config.contains("model_catalog_json = \"/opt/catalog.json\""));
    assert!(config.contains("shell_tool = false"));
    assert!(!config.contains("{model}") && !config.contains("{endpoint}"));
}

#[test]
fn a_catalog_cannot_be_written_without_codex() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("no-codex");
    assert!(write_codex_catalog(&missing, &dir.path().join("c.json"), "mock").is_err());
}
