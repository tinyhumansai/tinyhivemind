//! The names a package gives harnesses, and the wire form of the `harnesses`
//! section and the `harness` fields.
#![allow(clippy::unwrap_used)] // Tests fail loudly on unexpected results.
use super::{Capability, Harness, HarnessKind, Isolation, ModelApi, Support, Unlock};
use crate::{Manifest, Profile, RuntimeSettings, Seat};
use serde_json::json;

#[test]
fn each_kind_has_the_name_a_package_uses() -> Result<(), serde_json::Error> {
    for (kind, name) in [
        (HarnessKind::OpenHuman, "openhuman"),
        (HarnessKind::OpenClaw, "openclaw"),
        (HarnessKind::Codex, "codex"),
    ] {
        assert_eq!(serde_json::to_value(kind)?, json!(name));
        assert_eq!(serde_json::from_value::<HarnessKind>(json!(name))?, kind);
        assert_eq!(kind.to_string(), name);
    }
    for unknown in ["open_human", "OpenClaw", "claude", ""] {
        assert!(serde_json::from_value::<HarnessKind>(json!(unknown)).is_err());
    }
    Ok(())
}

#[test]
fn a_harness_entry_is_an_id_a_kind_and_an_optional_isolation() {
    let shared: Harness = serde_json::from_value(
        json!({"id": "claw", "kind": "openclaw", "isolation": "deployment"}),
    )
    .unwrap();
    assert_eq!(
        shared,
        Harness {
            id: "claw".into(),
            kind: HarnessKind::OpenClaw,
            isolation: Some(Isolation::Deployment),
        }
    );
    assert_eq!(
        serde_json::to_value(&shared).unwrap(),
        json!({"id": "claw", "kind": "openclaw", "isolation": "deployment"})
    );
    // An omitted isolation is not written back, and takes the kind's default.
    let codex: Harness = serde_json::from_value(json!({"id": "codex", "kind": "codex"})).unwrap();
    assert_eq!(codex.isolation, None);
    assert_eq!(
        serde_json::to_value(&codex).unwrap(),
        json!({"id": "codex", "kind": "codex"})
    );
    assert_eq!(codex.resolved_isolation(), Isolation::Seat);
    let claw: Harness = serde_json::from_value(json!({"id": "c", "kind": "openclaw"})).unwrap();
    assert_eq!(claw.resolved_isolation(), Isolation::Deployment);
    let seat = Harness {
        isolation: Some(Isolation::Seat),
        ..claw
    };
    assert_eq!(seat.resolved_isolation(), Isolation::Seat);
    assert_eq!(
        serde_json::to_value(Isolation::Seat).unwrap(),
        json!("seat")
    );
}

#[test]
fn a_harness_entry_rejects_unknown_fields_a_missing_kind_and_null() {
    for bad in [
        json!({"id": "c", "kind": "codex", "program": "/usr/bin/codex"}),
        json!({"id": "c"}),
        json!({"id": "c", "kind": "codex", "isolation": null}),
        json!({"id": "c", "kind": "codex", "isolation": "process"}),
    ] {
        assert!(
            serde_json::from_value::<Harness>(bad.clone()).is_err(),
            "{bad}"
        );
    }
}

/// A minimal manifest with one seat, extended by `extra`.
fn manifest(extra: &serde_json::Value) -> serde_json::Value {
    let mut manifest = json!({
        "$schema": "https://tinyhumans.ai/schemas/hive/v1",
        "version": 1,
        "id": "h",
        "name": "H",
        "seats": [{"id": "a", "role": "r", "prompt": "seats/a.md", "template": "t"}]
    });
    for (key, value) in extra.as_object().unwrap() {
        manifest[key] = value.clone();
    }
    manifest
}

#[test]
fn an_absent_harness_is_not_serialized_anywhere() {
    let decoded: Manifest = serde_json::from_value(manifest(&json!({}))).unwrap();
    let wire = serde_json::to_value(&decoded).unwrap();
    assert!(wire.get("harnesses").is_none());
    assert!(wire["runtime"].get("harness").is_none());
    assert!(wire["seats"][0].get("harness").is_none());
    let profile: Profile = serde_json::from_value(json!({"id": "p"})).unwrap();
    assert!(
        serde_json::to_value(&profile)
            .unwrap()
            .get("harness")
            .is_none()
    );
    assert!(
        serde_json::to_value(RuntimeSettings::default())
            .unwrap()
            .get("harness")
            .is_none()
    );
}

#[test]
fn a_named_harness_is_carried_on_runtime_profile_and_seat() {
    let mut input = manifest(&json!({
        "harnesses": [{"id": "codex", "kind": "codex"}],
        "runtime": {"harness": "codex"},
        "profiles": [{"id": "p", "harness": "codex"}]
    }));
    input["seats"][0]["harness"] = json!("codex");
    let decoded: Manifest = serde_json::from_value(input).unwrap();
    assert_eq!(decoded.harnesses[0].kind, HarnessKind::Codex);
    assert_eq!(decoded.runtime.harness.as_deref(), Some("codex"));
    assert_eq!(decoded.profiles[0].harness.as_deref(), Some("codex"));
    assert_eq!(decoded.seats[0].harness.as_deref(), Some("codex"));
    let wire = serde_json::to_value(&decoded).unwrap();
    assert_eq!(wire["harnesses"], json!([{"id": "codex", "kind": "codex"}]));
    assert_eq!(wire["runtime"]["harness"], "codex");
    assert_eq!(wire["profiles"][0]["harness"], "codex");
    assert_eq!(wire["seats"][0]["harness"], "codex");
    let seat: Seat = serde_json::from_value(wire["seats"][0].clone()).unwrap();
    assert_eq!(seat, decoded.seats[0]);
}

/// A written value must survive the round trip, so the two spellings of
/// "nothing" that would not, `null` and an empty list, are decode errors
/// rather than a silent omission.
#[test]
fn an_explicit_null_harness_or_an_empty_harnesses_list_does_not_decode() {
    let mut seat_null = manifest(&json!({}));
    seat_null["seats"][0]["harness"] = json!(null);
    for bad in [
        manifest(&json!({"harnesses": []})),
        manifest(&json!({"runtime": {"harness": null}})),
        manifest(&json!({"profiles": [{"id": "p", "harness": null}]})),
        seat_null,
    ] {
        assert!(
            serde_json::from_value::<Manifest>(bad.clone()).is_err(),
            "{bad}"
        );
    }
}

/// The table as the spec writes it, one row per capability and one column
/// per kind. Moving a row is a deliberate change to this literal, made in the
/// same change as the conformance test (or the decision) that moves it.
#[test]
fn the_capability_table_is_the_spec_table() {
    use Capability as C;
    use Support::{Harness as H, Host, Rejected};
    const T: Support = Rejected(Unlock::Conformance);
    const R: Support = Rejected(Unlock::Release);
    const D: Support = Rejected(Unlock::Spec);
    let table = [
        (C::Essentials, [H, H, H]),
        (C::OpenaiEndpoint, [H, H, H]),
        (C::OpenaiDefaultEndpoint, [T, T, T]),
        (C::Openrouter, [T, T, T]),
        (C::Anthropic, [T, T, T]),
        (C::InheritedProvider, [D, D, D]),
        (C::ProviderCredential, [T, T, T]),
        (C::ServedToolPolicy, [Host, Host, Host]),
        (C::NoSandbox, [H, H, H]),
        (C::Sandbox, [D, D, D]),
        (C::Autonomy, [Host, Host, Host]),
        (C::BuiltinToolGroups, [D, D, D]),
        (C::BuiltinToolRules, [R, D, D]),
        (C::BuiltinToolHooks, [D, D, D]),
        (C::McpServers, [Host, Host, Host]),
        (C::HostSessionStore, [D, D, D]),
        (C::HostMemoryEngine, [D, D, D]),
        (C::Subagents, [T, D, D]),
        (C::DefinitionBase, [T, D, D]),
        (C::Jev, [T, D, D]),
        (C::SessionTuning, [T, D, D]),
        (C::Workflows, [Host, Host, Host]),
        (C::Skills, [D, D, D]),
        (C::UserSkills, [D, D, D]),
        (C::Budget, [D, D, D]),
        (C::IterationLimit, [H, D, D]),
        (C::Temperature, [D, D, D]),
        (C::MaxTokens, [D, D, D]),
        (C::BarePrompt, [D, D, D]),
        (C::ToolLists, [D, D, D]),
        (C::SendPolicy, [D, D, D]),
        (C::StopHooks, [D, D, D]),
        (C::FactoryConfig, [D, D, D]),
        (C::AgentId, [D, D, D]),
        (C::Workspace, [D, D, D]),
        (C::DeploymentId, [D, D, D]),
        (C::Concurrency, [D, D, D]),
        (C::MaxAgents, [D, D, D]),
        (C::SqliteSessions, [D, D, D]),
        (C::InProcessMemory, [D, D, D]),
    ];
    assert_eq!(
        table.iter().map(|(c, _)| *c).collect::<Vec<_>>(),
        Capability::ALL
    );
    for (capability, row) in table {
        for (kind, support) in HarnessKind::ALL.into_iter().zip(row) {
            assert_eq!(kind.support(capability), support, "{kind} {capability}");
        }
    }
}

#[test]
fn only_a_rejected_row_is_unsupported_and_each_row_reads_as_prose() {
    assert!(Support::Harness.is_supported());
    assert!(Support::Host.is_supported());
    assert!(!Support::Rejected(Unlock::Conformance).is_supported());
    for capability in Capability::ALL {
        let prose = capability.to_string();
        assert!(
            !prose.is_empty() && prose == prose.to_lowercase(),
            "{prose}"
        );
    }
    assert_eq!(
        Capability::OpenaiDefaultEndpoint.to_string(),
        "provider openai without an endpoint"
    );
    assert_eq!(
        Unlock::Conformance.to_string(),
        "rejected until a conformance test on the pinned binary proves it"
    );
}

#[test]
fn each_kind_names_its_model_api_and_iteration_cap() {
    assert_eq!(
        HarnessKind::OpenHuman.model_api(),
        ModelApi::ChatCompletions
    );
    assert_eq!(HarnessKind::OpenClaw.model_api(), ModelApi::ChatCompletions);
    assert_eq!(HarnessKind::Codex.model_api(), ModelApi::Responses);
    assert_eq!(HarnessKind::OpenHuman.iteration_cap(), Some(8));
    assert_eq!(HarnessKind::OpenClaw.iteration_cap(), None);
    assert_eq!(HarnessKind::Codex.iteration_cap(), None);
    assert_eq!(
        HarnessKind::OpenClaw.default_isolation(),
        Isolation::Deployment
    );
    assert_eq!(HarnessKind::Codex.default_isolation(), Isolation::Seat);
}

/// The keys `value` serializes with.
fn wire_keys<T: serde::Serialize>(value: &T) -> Vec<String> {
    let mut keys: Vec<String> = serde_json::to_value(value)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

/// Every field of every integration type has a capability decision. A field
/// added to one of these types without an entry in `fields.rs` fails here,
/// so the default-deny judgement cannot drift from the language.
#[test]
fn every_field_the_language_has_has_a_capability_decision() {
    use crate::*;
    let harness = Some("h".to_owned());
    let mut manifest = Manifest::default();
    manifest.harnesses.push(Harness {
        id: "h".into(),
        kind: HarnessKind::Codex,
        isolation: Some(Isolation::Seat),
    });
    let tool: ExecTool =
        serde_json::from_value(json!({"id": "t", "description": "d", "command": ["t"]})).unwrap();
    let workflow: Workflow = serde_json::from_value(
        json!({"id": "w", "schedule": "* * * * *", "target": {"hive": "h"}, "prompt": "p"}),
    )
    .unwrap();
    let samples: Vec<(&str, Vec<String>)> = vec![
        ("manifest", wire_keys(&manifest)),
        (
            "runtime",
            wire_keys(&RuntimeSettings {
                harness: harness.clone(),
                ..RuntimeSettings::default()
            }),
        ),
        ("limits", wire_keys(&Limits::default())),
        ("provider", wire_keys(&Provider::default())),
        ("model", wire_keys(&Model::default())),
        ("mcp", wire_keys(&McpServer::default())),
        ("skill", wire_keys(&Skill::default())),
        ("tool", wire_keys(&tool)),
        ("toolpack", wire_keys(&Toolpack::default())),
        ("permissions", wire_keys(&PermissionSet::default())),
        (
            "profile",
            wire_keys(&Profile {
                harness: harness.clone(),
                ..Profile::default()
            }),
        ),
        ("tools", wire_keys(&ToolAccess::default())),
        ("hook", wire_keys(&Hook::default())),
        ("session", wire_keys(&SessionSettings::default())),
        ("jev", wire_keys(&JevSettings::default())),
        ("workflow", wire_keys(&workflow)),
        (
            "seat",
            wire_keys(&Seat {
                harness,
                ..Seat::default()
            }),
        ),
        ("harness", wire_keys(&manifest.harnesses[0])),
    ];
    let owners: Vec<&str> = samples.iter().map(|(owner, _)| *owner).collect();
    assert_eq!(super::fields::owners(), owners);
    for (owner, keys) in samples {
        assert_eq!(super::fields::keys(owner), keys, "{owner}");
        for key in &keys {
            if let Some(super::fields::Field::Nested(inner)) = super::fields::field(owner, key) {
                assert!(owners.contains(&inner), "{owner}.{key} nests {inner}");
            }
        }
    }
}
