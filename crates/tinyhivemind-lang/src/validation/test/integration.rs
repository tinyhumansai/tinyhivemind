//! Integration sections: the shipped package checks clean, and each rule
//! reports its own finding at its own pointer.
#![allow(clippy::unwrap_used)]

use crate::*;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// The `integrations` example package, every file keyed by relative path.
pub(crate) fn documents() -> BTreeMap<String, String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/hives/integrations");
    let mut files = BTreeMap::new();
    let mut pending = vec![(root, String::new())];
    while let Some((dir, prefix)) = pending.pop() {
        for entry in std::fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let key = format!("{prefix}{}", entry.file_name().to_string_lossy());
            if entry.file_type().unwrap().is_dir() {
                pending.push((entry.path(), format!("{key}/")));
            } else {
                files.insert(key, std::fs::read_to_string(entry.path()).unwrap());
            }
        }
    }
    files
}

/// The diagnostics after `edit` rewrites the manifest's JSON.
fn after(edit: impl FnOnce(&mut Value)) -> Vec<Diagnostic> {
    let mut docs = documents();
    let mut manifest: Value = serde_json::from_str(&docs["hive.json"]).unwrap();
    edit(&mut manifest);
    docs.insert("hive.json".into(), manifest.to_string());
    diagnose_documents(&docs).1
}

fn has(found: &[Diagnostic], rule: Rule, pointer: &str) -> bool {
    found
        .iter()
        .any(|d| d.rule == rule && d.pointer.as_deref() == Some(pointer))
}

#[test]
fn the_integrations_package_checks_clean_and_keeps_its_scripts() {
    let (package, found) = diagnose_documents(&documents());
    assert_eq!(found, vec![]);
    let package = package.unwrap();
    assert!(package.documents.contains_key("tools/grep_repo.sh"));
    assert!(package.documents.contains_key("tools/word_count.py"));
    assert!(
        package
            .documents
            .contains_key("skills/release-notes/SKILL.md")
    );
}

#[test]
fn an_undeclared_integration_is_an_unresolved_reference() {
    for (edit, pointer) in [
        (json!(["runtime", "provider"]), "/runtime/provider"),
        (json!(["runtime", "model"]), "/runtime/model"),
        (json!(["models", 1, "provider"]), "/models/1/provider"),
        (json!(["seats", 1, "model"]), "/seats/1/model"),
        (json!(["seats", 0, "profile"]), "/seats/0/profile"),
        (json!(["seats", 1, "permissions"]), "/seats/1/permissions"),
        (json!(["hooks", 0, "run"]), "/hooks/0/run"),
        (json!(["profiles", 1, "model"]), "/profiles/1/model"),
    ] {
        let found = after(|m| {
            let mut at = &mut *m;
            for key in edit.as_array().unwrap() {
                at = match key {
                    Value::String(key) => &mut at[key.as_str()],
                    Value::Number(i) => &mut at[usize::try_from(i.as_u64().unwrap()).unwrap()],
                    _ => unreachable!(),
                };
            }
            *at = json!("ghost");
        });
        assert!(
            has(&found, Rule::Reference, pointer),
            "{pointer}: {found:?}"
        );
    }
    let found = after(|m| m["toolpacks"][0]["tools"][0] = json!("ghost"));
    assert!(has(&found, Rule::Reference, "/toolpacks/0/tools/0"));
    let found = after(|m| m["seats"][0]["hooks"][0] = json!("ghost"));
    assert!(has(&found, Rule::Reference, "/seats/0/hooks/0"));
    let found = after(|m| m["profiles"][0]["subagents"][0] = json!("ghost"));
    assert!(has(&found, Rule::Reference, "/profiles/0/subagents/0"));
    let found = after(|m| m["workflows"][0]["target"] = json!({"hive": "elsewhere"}));
    assert!(has(&found, Rule::Reference, "/workflows/0/target"));
    let found = after(|m| m["workflows"][0]["target"] = json!({"seat": "ghost"}));
    assert!(has(&found, Rule::Reference, "/workflows/0/target"));
}

#[test]
fn integration_identities_are_unique_and_named() {
    let found = after(|m| {
        let copy = m["tools"][0].clone();
        m["tools"].as_array_mut().unwrap().push(copy);
    });
    assert!(has(&found, Rule::Identity, "/tools/3/id"));
    let found = after(|m| m["providers"][0]["id"] = json!(" "));
    assert!(has(&found, Rule::Identity, "/providers/0/id"));
}

#[test]
fn shipped_files_must_be_in_the_package() {
    let mut docs = documents();
    docs.remove("tools/grep_repo.sh");
    docs.remove("skills/release-notes/SKILL.md");
    let found = diagnose_documents(&docs).1;
    assert!(has(&found, Rule::MissingDocument, "/tools/0/command/0"));
    assert!(has(&found, Rule::MissingDocument, "/skills/0/id"));
    let found = after(|m| m["skills"][0]["id"] = json!("../escape"));
    assert!(has(&found, Rule::Path, "/skills/0/id"));
    let found = after(|m| m["seats"][0]["context"] = json!(["context/missing.md"]));
    assert!(has(&found, Rule::MissingDocument, "/seats/0/context/0"));
}

#[test]
fn zero_and_negative_settings_are_out_of_bounds() {
    for (edit, pointer) in [
        (json!(["runtime", "concurrency", 0]), "/runtime/concurrency"),
        (json!(["runtime", "max_agents", 0]), "/runtime/max_agents"),
        (json!(["jev", "top_k", 0]), "/jev/top_k"),
        (
            json!(["session", "max_history_messages", 0]),
            "/session/max_history_messages",
        ),
        (json!(["tools", 0, "timeout_ms", 0]), "/tools/0/timeout_ms"),
        (json!(["hooks", 0, "timeout_ms", 0]), "/hooks/0/timeout_ms"),
        (
            json!(["models", 0, "max_tokens", 0]),
            "/models/0/max_tokens",
        ),
        (
            json!(["models", 0, "temperature", -1.0]),
            "/models/0/temperature",
        ),
        (
            json!(["workflows", 0, "schedule", "daily"]),
            "/workflows/0/schedule",
        ),
    ] {
        let path = edit.as_array().unwrap();
        let found = after(|m| {
            let (value, keys) = path.split_last().unwrap();
            let mut at = &mut *m;
            for key in keys {
                at = match key {
                    Value::String(key) => &mut at[key.as_str()],
                    Value::Number(i) => &mut at[usize::try_from(i.as_u64().unwrap()).unwrap()],
                    _ => unreachable!(),
                };
            }
            *at = value.clone();
        });
        assert!(has(&found, Rule::Bound, pointer), "{pointer}: {found:?}");
    }
    let found = after(|m| m["runtime"]["limits"]["budget_usd"] = json!(-0.5));
    assert!(has(&found, Rule::Bound, "/runtime/limits/budget_usd"));
    let found = after(|m| m["seats"][1]["limits"]["iterations"] = json!(0));
    assert!(has(&found, Rule::Bound, "/seats/1/limits/iterations"));
}

#[test]
fn unrunnable_shapes_are_integration_findings() {
    let found = after(|m| m["tools"][0]["argv"] = json!(["{nope}"]));
    assert!(has(&found, Rule::Integration, "/tools/0/argv/0"));
    let found = after(|m| m["tools"][0]["args"] = json!({"type": "string"}));
    assert!(has(&found, Rule::Integration, "/tools/0/args"));
    let found = after(|m| m["tools"][0]["id"] = json!("Grep-Repo"));
    assert!(has(&found, Rule::Integration, "/tools/0/id"));
    let found = after(|m| m["tools"][0]["command"] = json!([]));
    assert!(has(&found, Rule::Integration, "/tools/0/command"));
    let found = after(|m| m["tools"][0]["env"] = json!({"TOKEN": {"env": ""}}));
    assert!(has(&found, Rule::Integration, "/tools/0/env/TOKEN"));
    let found = after(|m| m["providers"][0]["credential"] = json!({"store": " "}));
    assert!(has(&found, Rule::Integration, "/providers/0/credential"));
    let found = after(|m| m["mcp"][0]["endpoint"] = json!(""));
    assert!(has(&found, Rule::Integration, "/mcp/0/endpoint"));
    let found = after(|m| m["permissions"][0]["allow_tools"] = json!(["shell"]));
    assert!(has(
        &found,
        Rule::Integration,
        "/permissions/0/deny_tools/0"
    ));
    let found = after(|m| m["hooks"][0]["event"] = json!("stop"));
    assert!(has(&found, Rule::Integration, "/hooks/0/matcher"));
    let found = after(|m| m["runtime"]["session_store"] = json!("sqlite"));
    assert!(has(&found, Rule::Integration, "/runtime/session_store"));
    let found = after(|m| m["features"] = json!({"Bad-Flag": true}));
    assert!(has(&found, Rule::Integration, "/features/Bad-Flag"));
}

#[test]
fn a_bare_string_secret_or_an_sse_transport_does_not_decode() {
    let found = after(|m| m["providers"][0]["credential"] = json!("sk-live"));
    assert!(found.iter().any(|d| d.rule == Rule::Json));
    let found = after(|m| m["mcp"][0]["transport"] = json!("sse"));
    assert!(found.iter().any(|d| d.rule == Rule::Json));
}

#[test]
fn pins_the_integration_wire() {
    assert_eq!(
        serde_json::to_value(SecretRef::Env("KEY".into())).unwrap(),
        json!({"env": "KEY"})
    );
    assert_eq!(
        serde_json::to_value(WorkflowTarget::Seat("lead".into())).unwrap(),
        json!({"seat": "lead"})
    );
    let tool: ExecTool = serde_json::from_value(json!({
        "id": "t", "description": "d", "command": ["tools/t.sh"]
    }))
    .unwrap();
    assert_eq!(
        serde_json::to_value(&tool).unwrap(),
        json!({"id":"t","description":"d","command":["tools/t.sh"],"args":{"type":"object"},"argv":[],"stdin":"json","output":"text","timeout_ms":60000,"env":{},"side_effects":"write","cwd":"workspace"})
    );
    assert_eq!(tool.package_program(), Some("tools/t.sh"));
    assert_eq!(placeholder("{a?}"), Some(("a", true)));
    assert_eq!(placeholder("{a}"), Some(("a", false)));
    assert_eq!(placeholder("a"), None);
    assert_eq!(
        serde_json::to_value(JevSettings::default()).unwrap(),
        json!({"ranker":"jev","route":"auto","base_url":null,"top_k":3})
    );
    let limits = Limits {
        iterations: Some(2),
        ..Limits::default()
    };
    let base = Limits {
        timeout_ms: Some(5),
        iterations: Some(9),
        budget_usd: None,
    };
    assert_eq!(
        limits.or(&base),
        Limits {
            timeout_ms: Some(5),
            iterations: Some(2),
            budget_usd: None
        }
    );
}
