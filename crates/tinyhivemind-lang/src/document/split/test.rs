//! Unit tests for the split files: where each section lives, and which
//! file a manifest finding belongs in.
#![allow(clippy::expect_used)] // Tests fail loudly on unexpected results.
use super::*;

fn keys<T: Serialize>(value: &T) -> Vec<String> {
    let mut keys: Vec<String> = serde_json::to_value(value)
        .expect("serializes")
        .as_object()
        .expect("an object")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys
}

fn sections(file: &str) -> Vec<String> {
    let (_, sections) = SPLITS.iter().find(|(f, _)| *f == file).expect("a split");
    let mut sections: Vec<String> = sections.iter().map(|s| (*s).to_owned()).collect();
    sections.sort();
    sections
}

#[test]
fn each_typed_split_file_holds_exactly_its_sections() {
    let empty = "{}";
    // `harnesses` is written only when it holds an entry.
    let agents: AgentsFile =
        serde_json::from_str(r#"{"harnesses": [{"id": "c", "kind": "codex"}]}"#).expect("decodes");
    let tools: ToolsFile = serde_json::from_str(empty).expect("decodes");
    let desks: DesksFile = serde_json::from_str(empty).expect("decodes");
    assert_eq!(keys(&agents), sections(AGENTS));
    assert_eq!(keys(&tools), sections(TOOLS));
    assert_eq!(keys(&desks), sections(DESKS));
}

#[test]
fn every_split_section_is_a_manifest_section_owned_by_one_file() {
    let mut manifest = super::super::Manifest::default();
    manifest.harnesses.push(crate::Harness {
        id: "c".into(),
        kind: crate::HarnessKind::Codex,
        isolation: None,
    });
    let manifest = keys(&manifest);
    let mut all: Vec<&str> = SPLITS.iter().flat_map(|(_, s)| s.iter().copied()).collect();
    let count = all.len();
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), count, "a section is owned twice");
    assert!(all.iter().all(|s| manifest.iter().any(|m| m == s)));
}

#[test]
fn a_finding_moves_only_into_a_present_split_file_that_owns_its_section() {
    let present = BTreeMap::from([(AGENTS.to_owned(), String::new())]);
    assert_eq!(home(HIVE, Some("/seats/0/role"), &present), Some(AGENTS));
    assert_eq!(home(HIVE, Some("/roles"), &present), Some(AGENTS));
    // Its file is absent, so the desk section is still in hive.json.
    assert_eq!(home(HIVE, Some("/desks/0"), &present), None);
    assert_eq!(home(HIVE, Some("/goals/0"), &present), None);
    assert_eq!(home(HIVE, None, &present), None);
    assert_eq!(home("memory.json", Some("/seats"), &present), None);
}

#[test]
fn harnesses_live_in_agents_json_and_are_reported_there() {
    let mut documents = crate::validation::test_example("mixed-harnesses");
    let mut hive: serde_json::Value =
        serde_json::from_str(&documents[HIVE]).expect("the manifest decodes");
    let mut harnesses = hive
        .as_object_mut()
        .expect("an object")
        .remove("harnesses")
        .expect("the example declares harnesses");
    let whole = crate::parse(&documents).expect("the example parses");
    documents.insert(HIVE.into(), hive.to_string());
    documents.insert(
        AGENTS.into(),
        serde_json::json!({ "harnesses": harnesses }).to_string(),
    );
    assert_eq!(crate::parse(&documents).expect("the split parses"), whole);
    harnesses[1]["isolation"] = "deployment".into();
    documents.insert(
        AGENTS.into(),
        serde_json::json!({ "harnesses": harnesses }).to_string(),
    );
    let (_, found) = crate::diagnose_documents(&documents);
    assert!(
        found
            .iter()
            .any(|d| d.file == AGENTS && d.pointer.as_deref() == Some("/harnesses/1/isolation")),
        "{found:?}"
    );
}
