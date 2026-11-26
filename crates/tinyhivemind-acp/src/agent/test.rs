//! Descriptors and their templates.
#![allow(clippy::unwrap_used, clippy::panic)] // Tests fail loudly on unexpected results.
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
fn a_value_is_never_expanded_again() {
    let mut values = values();
    values
        .values
        .insert("catalog".into(), "/tmp/{model}/catalog.json".into());
    assert_eq!(
        values.fill("{catalog} for {model}"),
        "/tmp/{model}/catalog.json for mock"
    );
    assert_eq!(values.fill("{{model}} {model"), "{mock} {model");
}

#[test]
fn a_toml_file_gets_its_values_escaped() {
    let mut values = values();
    values
        .values
        .insert("catalog".into(), "/tmp/a\"b\\c\n\u{1}.json".into());
    let template = "model_catalog_json = \"{catalog}\"";
    let filled = values.fill_file(Path::new("codex/config.toml"), template);
    assert_eq!(
        filled,
        "model_catalog_json = \"/tmp/a\\\"b\\\\c\\n\\u0001.json\""
    );
    assert_eq!(
        values.fill_file(Path::new("notes.txt"), "{catalog}"),
        "/tmp/a\"b\\c\n\u{1}.json"
    );
}

#[cfg(unix)]
#[test]
fn each_catalog_failure_says_which_it_was() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("c.json");
    let script = |name: &str, body: &str| {
        let path = dir.path().join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    };
    let missing = dir.path().join("no-codex");
    assert!(matches!(
        write_codex_catalog(&missing, &out, "mock"),
        Err(CatalogError::Launch(_))
    ));
    let failing = script("failing", "echo broken >&2; exit 3");
    match write_codex_catalog(&failing, &out, "mock") {
        Err(CatalogError::Exit { stderr, .. }) => assert_eq!(stderr, "broken"),
        other => panic!("{other:?}"),
    }
    let text = script("text", "echo not json");
    assert!(matches!(
        write_codex_catalog(&text, &out, "mock"),
        Err(CatalogError::Json(_))
    ));
    let empty = script("empty", "echo '{}'");
    assert!(matches!(
        write_codex_catalog(&empty, &out, "mock"),
        Err(CatalogError::NoModels)
    ));
    let models = script("models", r#"echo '{"models": [{"slug": "other"}]}'"#);
    let unwritable = dir.path().join("missing-dir").join("c.json");
    assert!(matches!(
        write_codex_catalog(&models, &unwritable, "mock"),
        Err(CatalogError::Write(_))
    ));
    write_codex_catalog(&models, &out, "mock").unwrap();
    let catalog: Value = serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
    assert_eq!(catalog["models"][0]["slug"], "mock");
    assert_eq!(catalog["models"][1]["tool_mode"], "direct");
}
