//! The names a package gives harnesses, and the wire form of the `harnesses`
//! section and the `harness` fields.
#![allow(clippy::unwrap_used)] // Tests fail loudly on unexpected results.
use super::{Harness, HarnessKind, Isolation};
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
fn manifest(extra: serde_json::Value) -> serde_json::Value {
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
    let decoded: Manifest = serde_json::from_value(manifest(json!({}))).unwrap();
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
    let mut input = manifest(json!({
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
    let mut seat_null = manifest(json!({}));
    seat_null["seats"][0]["harness"] = json!(null);
    for bad in [
        manifest(json!({"harnesses": []})),
        manifest(json!({"runtime": {"harness": null}})),
        manifest(json!({"profiles": [{"id": "p", "harness": null}]})),
        seat_null,
    ] {
        assert!(
            serde_json::from_value::<Manifest>(bad.clone()).is_err(),
            "{bad}"
        );
    }
}
