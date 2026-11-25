//! The `harnesses` section and the `harness` fields: references, identities
//! and isolation, and the capability table judged at each field's location.
#![allow(clippy::unwrap_used)] // Tests fail loudly on unexpected results.

use super::integration::{example, has};
use crate::*;
use serde_json::{Value, json};

/// The diagnostics of example `name` after `edit` rewrites its manifest.
fn after(name: &str, edit: impl FnOnce(&mut Value)) -> Vec<Diagnostic> {
    let mut docs = example(name);
    let mut manifest: Value = serde_json::from_str(&docs["hive.json"]).unwrap();
    edit(&mut manifest);
    docs.insert("hive.json".into(), manifest.to_string());
    diagnose_documents(&docs).1
}

/// The diagnostics of `mixed-harnesses` after `edit`.
fn mixed(edit: impl FnOnce(&mut Value)) -> Vec<Diagnostic> {
    after("mixed-harnesses", edit)
}

/// The pointers of every `integration` finding.
fn integration_pointers(found: &[Diagnostic]) -> Vec<&str> {
    found
        .iter()
        .filter(|d| d.rule == Rule::Integration)
        .filter_map(|d| d.pointer.as_deref())
        .collect()
}

#[test]
fn the_mixed_harnesses_package_checks_clean_with_a_harness_per_seat() {
    let (package, found) = diagnose_documents(&example("mixed-harnesses"));
    assert_eq!(found, vec![]);
    let hive = lower(&package.unwrap()).unwrap();
    let kinds: Vec<_> = hive
        .seats
        .iter()
        .map(|seat| (seat.id.as_str(), seat.integration.harness_kind()))
        .collect();
    assert_eq!(
        kinds,
        [
            ("lead", HarnessKind::OpenHuman),
            ("scout", HarnessKind::OpenClaw),
            ("builder", HarnessKind::Codex),
        ]
    );
    assert_eq!(hive.seats[0].integration.harness, None);
    let claw = hive.seats[1].integration.harness.as_ref().unwrap();
    assert_eq!(claw.resolved_isolation(), Isolation::Deployment);
}

#[test]
fn an_undeclared_harness_is_an_unresolved_reference() {
    let found = mixed(|m| m["runtime"]["harness"] = json!("ghost"));
    assert!(
        has(&found, Rule::Reference, "/runtime/harness"),
        "{found:?}"
    );
    let found = mixed(|m| m["profiles"][0]["harness"] = json!("ghost"));
    assert!(has(&found, Rule::Reference, "/profiles/0/harness"));
    let found = mixed(|m| m["seats"][1]["harness"] = json!("ghost"));
    assert!(has(&found, Rule::Reference, "/seats/1/harness"));
    // With no `harnesses` section at all, a named harness still resolves
    // nowhere.
    let found = after("integrations", |m| {
        m["seats"][0]["harness"] = json!("codex")
    });
    assert!(has(&found, Rule::Reference, "/seats/0/harness"));
}

#[test]
fn harness_ids_are_unique_and_named() {
    let found = mixed(|m| {
        let copy = m["harnesses"][1].clone();
        m["harnesses"].as_array_mut().unwrap().push(copy);
    });
    assert!(has(&found, Rule::Identity, "/harnesses/2/id"), "{found:?}");
    let found = mixed(|m| m["harnesses"][0]["id"] = json!(" "));
    assert!(has(&found, Rule::Identity, "/harnesses/0/id"));
}

#[test]
fn only_openclaw_shares_an_instance_across_a_deployment() {
    for kind in ["codex", "openhuman"] {
        let found = mixed(|m| {
            m["harnesses"][1] = json!({"id": "codex", "kind": kind, "isolation": "deployment"});
        });
        assert!(
            has(&found, Rule::Integration, "/harnesses/1/isolation"),
            "{kind}: {found:?}"
        );
    }
    let found = mixed(|m| m["harnesses"][1]["isolation"] = json!("seat"));
    assert_eq!(found, vec![]);
    let found = mixed(|m| m["harnesses"][0]["isolation"] = json!("seat"));
    assert_eq!(found, vec![]);
}

#[test]
fn a_delegated_profile_may_not_name_a_harness() {
    let found = after("integrations", |m| {
        m["harnesses"] = json!([{"id": "oh", "kind": "openhuman"}]);
        m["profiles"][2]["harness"] = json!("oh");
    });
    assert!(
        has(&found, Rule::Integration, "/profiles/2/harness"),
        "{found:?}"
    );
}

#[test]
fn capabilities_are_judged_only_once_a_package_names_a_harness() {
    // `integrations` sets a hosted provider with a credential, a sandbox,
    // session tuning, Jev and a delegate: all fine on the linked adapter.
    assert_eq!(after("integrations", |_| {}), vec![]);
    let found = after("integrations", |m| {
        m["harnesses"] = json!([{"id": "oh", "kind": "openhuman"}]);
    });
    let pointers = integration_pointers(&found);
    for pointer in [
        "/providers/0/kind",
        "/providers/0/credential",
        "/permissions/0/sandbox",
        "/session/compact_context",
        "/jev",
        "/profiles/0/subagents",
    ] {
        assert!(pointers.contains(&pointer), "{pointer}: {found:?}");
    }
}

#[test]
fn every_rejected_provider_row_is_reported_at_the_provider() {
    for (edit, pointer) in [
        (json!({"endpoint": null}), "/providers/0/endpoint"),
        (json!({"kind": "openrouter"}), "/providers/0/kind"),
        (json!({"kind": "anthropic"}), "/providers/0/kind"),
        (json!({"kind": "inherit"}), "/providers/0/kind"),
        (
            json!({"credential": {"env": "KEY"}}),
            "/providers/0/credential",
        ),
    ] {
        let found = mixed(|m| {
            for (key, value) in edit.as_object().unwrap() {
                m["providers"][0][key] = value.clone();
            }
        });
        assert_eq!(integration_pointers(&found), [pointer], "{edit}");
    }
}

#[test]
fn a_seat_that_resolves_to_no_provider_is_rejected() {
    let found = mixed(|m| {
        m["models"][0]["provider"] = json!(null);
        m["runtime"]["provider"] = json!(null);
    });
    assert_eq!(integration_pointers(&found), ["/models/0/provider"]);
    let found = mixed(|m| {
        m["runtime"]["model"] = json!(null);
        m["runtime"]["provider"] = json!(null);
    });
    assert_eq!(integration_pointers(&found), ["/runtime/provider"]);
    // Without a model, the runtime's provider is the seat's.
    let found = mixed(|m| m["runtime"]["model"] = json!(null));
    assert_eq!(found, vec![]);
}

#[test]
fn a_sandbox_and_built_in_tool_settings_are_rejected() {
    for (edit, pointer) in [
        (
            json!(["runtime", "sandbox", "read_only"]),
            "/runtime/sandbox",
        ),
        (
            json!(["permissions", 0, "sandbox", "sandboxed"]),
            "/permissions/0/sandbox",
        ),
        (
            json!(["permissions", 0, "tool_rules", {"shell": "deny"}]),
            "/permissions/0/tool_rules",
        ),
        (
            json!(["toolpacks", 0, "groups", {"web3": "advertised"}]),
            "/toolpacks/0/groups/web3",
        ),
        (
            json!(["runtime", "session_store", "host"]),
            "/runtime/session_store",
        ),
        (
            json!(["runtime", "memory_engine", "host"]),
            "/runtime/memory_engine",
        ),
        (
            json!(["profiles", 0, "definition_base", "coder"]),
            "/profiles/0/definition_base",
        ),
        (json!(["jev", "top_k", 5]), "/jev"),
        (
            json!(["session", "max_tool_iterations", 4]),
            "/session/max_tool_iterations",
        ),
    ] {
        let path = edit.as_array().unwrap();
        let found = mixed(|m| {
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
        assert_eq!(integration_pointers(&found), [pointer], "{edit}");
    }
    // A group turned off is the state every kind is already in.
    let found = mixed(|m| m["toolpacks"][0]["groups"] = json!({"web3": "off"}));
    assert_eq!(found, vec![]);
}

#[test]
fn a_delegate_is_rejected_on_the_profile_that_names_it() {
    let found = mixed(|m| {
        m["profiles"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id": "helper"}));
        m["profiles"][0]["subagents"] = json!(["helper"]);
    });
    assert_eq!(integration_pointers(&found), ["/profiles/0/subagents"]);
}

#[test]
fn openhuman_rejects_more_iterations_than_its_fixed_cap() {
    let found = mixed(|m| m["seats"][0]["limits"]["iterations"] = json!(9));
    assert_eq!(integration_pointers(&found), ["/seats/0/limits/iterations"]);
    // The cap is OpenHuman's: the runtime default reaches every seat, and
    // only the `openhuman` one reports it.
    let found = mixed(|m| m["runtime"]["limits"]["iterations"] = json!(20));
    assert_eq!(integration_pointers(&found), ["/runtime/limits/iterations"]);
    let found = mixed(|m| m["runtime"]["limits"]["iterations"] = json!(8));
    assert_eq!(found, vec![]);
}

#[test]
fn a_shared_openclaw_instance_runs_one_provider() {
    let found = mixed(|m| {
        m["providers"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id": "other", "kind": "openai", "endpoint": "http://127.0.0.1:9090/v1"}));
        m["models"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id": "second", "provider": "other", "name": "second-model"}));
        m["seats"][0]["harness"] = json!("claw");
        m["seats"][0]["model"] = json!("second");
    });
    assert_eq!(integration_pointers(&found), ["/harnesses/0/isolation"]);
    let found = mixed(|m| {
        m["providers"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id": "other", "kind": "openai", "endpoint": "http://127.0.0.1:9090/v1"}));
        m["models"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id": "second", "provider": "other", "name": "second-model"}));
        m["seats"][0]["harness"] = json!("claw");
        m["seats"][0]["model"] = json!("second");
        m["harnesses"][0]["isolation"] = json!("seat");
    });
    assert_eq!(found, vec![]);
}

#[test]
fn an_executable_tool_may_not_declare_the_reserved_turn_or_call_arguments() {
    for name in ["turn", "call"] {
        let found = mixed(|m| {
            m["tools"][0]["args"]["properties"][name] = json!({"type": "string"});
        });
        let pointer = format!("/tools/0/args/properties/{name}");
        assert_eq!(integration_pointers(&found), [pointer.as_str()]);
    }
}

#[test]
fn a_seat_prompt_frontmatter_may_restate_its_own_harness_only() {
    let with = |frontmatter: &str| {
        let mut docs = example("mixed-harnesses");
        let body = docs["seats/scout.md"].clone();
        docs.insert(
            "seats/scout.md".into(),
            format!("---\n{frontmatter}\n---\n{body}"),
        );
        diagnose_documents(&docs).1
    };
    assert_eq!(with(r#"{"id": "scout", "harness": "claw"}"#), vec![]);
    for wrong in [
        r#"{"harness": "codex"}"#,
        // The seat has a harness; frontmatter cannot unset it.
        r#"{"harness": null}"#,
    ] {
        let found = with(wrong);
        assert!(
            found
                .iter()
                .any(|d| d.rule == Rule::Frontmatter && d.file == "seats/scout.md"),
            "{wrong}: {found:?}"
        );
    }
    // A seat without a harness has no `harness` key to restate, not even null.
    let mut docs = example("mixed-harnesses");
    let body = docs["seats/lead.md"].clone();
    docs.insert(
        "seats/lead.md".into(),
        format!("---\n{{\"harness\": null}}\n---\n{body}"),
    );
    let found = diagnose_documents(&docs).1;
    assert!(found.iter().any(|d| d.rule == Rule::Frontmatter), "{found:?}");
}
