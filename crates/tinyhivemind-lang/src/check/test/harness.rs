//! Editing what a seat runs on: `harnesses` and `runtime.harness` are
//! integration edits, and moving a seat to another harness needs the
//! integration class as well as roster.
use crate::{patch::PatchClass, *};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// `mixed-harnesses`, its constitution permitting exactly `permitted`.
fn original(permitted: &[PatchClass]) -> BTreeMap<String, String> {
    let mut docs = crate::validation::test_example("mixed-harnesses");
    let mut constitution: Constitution =
        serde_json::from_str(&docs["constitution.json"]).expect("constitution decodes");
    constitution.permitted_edits = permitted.to_vec();
    docs.insert(
        "constitution.json".into(),
        serde_json::to_string(&constitution).expect("constitution encodes"),
    );
    docs
}

/// `base` with its manifest rewritten by `edit`.
fn edited(
    base: &BTreeMap<String, String>,
    edit: impl FnOnce(&mut Value),
) -> BTreeMap<String, String> {
    let mut docs = base.clone();
    let mut manifest: Value = serde_json::from_str(&docs["hive.json"]).expect("manifest decodes");
    edit(&mut manifest);
    docs.insert("hive.json".into(), manifest.to_string());
    docs
}

fn refusals(found: &[Diagnostic]) -> Vec<(Rule, Option<&str>)> {
    found
        .iter()
        .map(|d| (d.rule, d.pointer.as_deref()))
        .collect()
}

#[test]
fn harnesses_and_runtime_harness_are_integration_edits() {
    let base = original(&[PatchClass::Roster]);
    let edit = edited(&base, |m| m["harnesses"][1]["kind"] = json!("openclaw"));
    let refused = check(&base, &edit).expect_err("integration is not permitted");
    assert_eq!(
        refusals(&refused),
        [(Rule::ForbiddenClass, Some("/harnesses/1/kind"))]
    );
    let edit = edited(&base, |m| m["runtime"]["harness"] = json!("codex"));
    let refused = check(&base, &edit).expect_err("integration is not permitted");
    assert!(
        refusals(&refused).contains(&(Rule::ForbiddenClass, Some("/runtime/harness"))),
        "{refused:?}"
    );
    let base = original(&[PatchClass::Integration]);
    let edit = edited(&base, |m| m["runtime"]["harness"] = json!("codex"));
    let review = check(&base, &edit).expect("integration is permitted");
    assert_eq!(review.changes.len(), 1);
    assert_eq!(review.changes[0].class, PatchClass::Integration);
}

#[test]
fn a_roster_only_edit_cannot_move_a_seat_to_another_harness() {
    let base = original(&[PatchClass::Roster]);
    for (edit, pointer) in [
        (json!(["seats", 1, "harness", "codex"]), "/seats/1/harness"),
        (json!(["seats", 0, "harness", "claw"]), "/seats/0/harness"),
        // Through a profile that names another harness.
        (json!(["seats", 0, "profile", "coder"]), "/seats/0/profile"),
    ] {
        let path = edit.as_array().expect("an edit path");
        let edit = edited(&base, |m| {
            m[path[0].as_str().expect("a key")]
                [usize::try_from(path[1].as_u64().expect("an index")).expect("an index")]
                [path[2].as_str().expect("a key")] = path[3].clone();
        });
        let refused = check(&base, &edit).expect_err("a roster edit cannot move a seat");
        assert!(
            refusals(&refused).contains(&(Rule::ForbiddenClass, Some(pointer))),
            "{pointer}: {refused:?}"
        );
    }
    // Dropping a seat's harness moves it to `openhuman`.
    let edit = edited(&base, |m| {
        m["seats"][1]
            .as_object_mut()
            .expect("a seat")
            .remove("harness");
    });
    let refused = check(&base, &edit).expect_err("a roster edit cannot move a seat");
    assert!(refusals(&refused).contains(&(Rule::ForbiddenClass, Some("/seats/1/harness"))));
    // A roster edit that leaves every harness alone is a roster edit.
    let edit = edited(&base, |m| m["seats"][1]["label"] = json!("Field scout"));
    let review = check(&base, &edit).expect("a plain roster edit");
    assert_eq!(review.changes[0].class, PatchClass::Roster);
}

#[test]
fn a_new_seat_may_take_the_default_harness_but_not_name_another() {
    let base = original(&[PatchClass::Roster]);
    let seat = |harness: Option<&str>| {
        let base = base.clone();
        let mut edit = edited(&base, |m| {
            let mut seat = json!({"id": "second", "role": "builder",
                "prompt": "seats/second.md", "template": "default",
                "tools": {"allow": [], "deny": ["web_search", "web_fetch", "browser", "http"]}});
            if let Some(harness) = harness {
                seat["harness"] = json!(harness);
            }
            m["seats"].as_array_mut().expect("seats").push(seat);
        });
        edit.insert("seats/second.md".into(), "Draft a second option.\n".into());
        check(&base, &edit)
    };
    assert!(seat(None).is_ok());
    let refused = seat(Some("codex")).expect_err("naming a harness needs integration");
    assert!(refusals(&refused).contains(&(Rule::ForbiddenClass, Some("/seats/3/harness"))));
}

#[test]
fn with_integration_permitted_a_roster_edit_may_move_a_seat() {
    let base = original(&[PatchClass::Roster, PatchClass::Integration]);
    let edit = edited(&base, |m| m["seats"][1]["harness"] = json!("codex"));
    let review = check(&base, &edit).expect("both classes are permitted");
    assert_eq!(review.changes.len(), 1);
    assert_eq!(review.changes[0].class, PatchClass::Roster);
    assert_eq!(
        review.candidate.manifest.seats[1].harness.as_deref(),
        Some("codex")
    );
}

#[test]
fn a_recorded_roster_patch_that_moves_a_seat_is_refused_without_integration() {
    let original_docs = original(&[PatchClass::Roster]);
    let base = parse(&original_docs).expect("the original parses");
    let mut moved = base.clone();
    moved.manifest.seats[1].harness = Some("codex".into());
    assert!(matches!(
        patch::diff(&base, &moved, PatchClass::Roster),
        Err(patch::Error::ForbiddenClass)
    ));
    let mut renamed = base.clone();
    renamed.manifest.seats[1].label = "Field scout".into();
    assert!(patch::diff(&base, &renamed, PatchClass::Roster).is_ok());
}
