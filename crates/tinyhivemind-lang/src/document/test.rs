//! Collecting parse findings: every canonical file, located decode errors,
//! and agreement with `parse`.
#![allow(clippy::expect_used)] // Tests fail loudly on unexpected results.
use super::*;
use crate::{Constitution, MemorySpec, Rule};
use serde_json::json;

fn documents() -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "hive.json".to_owned(),
            serde_json::to_string_pretty(&Manifest::default()).expect("serializes"),
        ),
        (
            "constitution.json".to_owned(),
            serde_json::to_string(&Constitution::default()).expect("serializes"),
        ),
        (
            "memory.json".to_owned(),
            serde_json::to_string(&MemorySpec::default()).expect("serializes"),
        ),
    ])
}

fn rules(diagnostics: &[Diagnostic]) -> Vec<(&str, Option<&str>, Rule)> {
    diagnostics
        .iter()
        .map(|d| (d.file.as_str(), d.pointer.as_deref(), d.rule))
        .collect()
}

#[test]
fn a_valid_map_decodes_with_no_findings() {
    let (package, diagnostics) = diagnose_documents(&documents());
    assert_eq!(package, Some(Package::default()));
    assert_eq!((diagnostics).len(), 0);
}

#[test]
fn reports_every_missing_canonical_file_and_unsafe_path() {
    let docs = BTreeMap::from([("../x.md".to_owned(), String::new())]);
    let (package, diagnostics) = diagnose_documents(&docs);
    assert!(package.is_none());
    assert_eq!(
        rules(&diagnostics),
        [
            ("../x.md", None, Rule::Path),
            ("hive.json", None, Rule::MissingDocument),
            ("constitution.json", None, Rule::MissingDocument),
            ("memory.json", None, Rule::MissingDocument),
        ]
    );
    assert_eq!(
        parse(&docs),
        Err(Error::Path {
            path: "../x.md".into()
        })
    );
}

#[test]
fn a_schema_error_is_located_by_line_and_column() {
    let mut docs = documents();
    let mut hive: serde_json::Value = serde_json::from_str(&docs["hive.json"]).expect("decodes");
    hive["surprise"] = json!(true);
    docs.insert(
        "hive.json".into(),
        serde_json::to_string_pretty(&hive).expect("serializes"),
    );
    let (_, diagnostics) = diagnose_documents(&docs);
    assert_eq!(rules(&diagnostics), [("hive.json", None, Rule::Json)]);
    assert!(diagnostics[0].message.contains("unknown field `surprise`"));
    assert!(diagnostics[0].message.contains(" at line "));
}

#[test]
fn an_unknown_nested_field_is_reported_at_its_pointer() {
    let mut docs = documents();
    let mut hive: serde_json::Value = serde_json::from_str(&docs["hive.json"]).expect("decodes");
    hive["policies"]["approval"]["rules"] = json!([]);
    hive["desks"] = json!([
        {"id": "a", "name": "A", "description": null, "members": [], "responder_mode": "lead"},
        {"id": "b", "name": "B", "description": null, "members": [], "responder_mode": "auto", "x": 1}
    ]);
    docs.insert("hive.json".into(), hive.to_string());
    let (package, diagnostics) = diagnose_documents(&docs);
    assert!(package.is_none());
    assert_eq!(
        rules(&diagnostics),
        [("hive.json", Some("/desks/1/x"), Rule::Json)]
    );
    assert_eq!(
        diagnostics[0].message,
        "invalid json in hive.json: unknown nested schema field"
    );
}

#[test]
fn a_decoded_but_invalid_package_is_returned_with_its_findings() {
    let mut docs = documents();
    let manifest = Manifest {
        id: String::new(),
        version: 3,
        ..Manifest::default()
    };
    docs.insert(
        "hive.json".into(),
        serde_json::to_string(&manifest).expect("serializes"),
    );
    let (package, diagnostics) = diagnose_documents(&docs);
    assert!(package.is_some());
    assert_eq!(
        rules(&diagnostics),
        [
            ("hive.json", Some("/version"), Rule::Schema),
            ("hive.json", Some("/id"), Rule::Identity),
        ]
    );
    assert_eq!(parse(&docs), Err(Error::Schema));
}

#[test]
fn an_argv_template_names_its_arguments_and_which_are_optional() {
    let tool: ExecTool = serde_json::from_value(json!({
        "id": "t", "description": "d", "command": ["tools/run.sh"],
        "argv": ["--in", "{path}", "{limit?}", "{}", "{not closed", "x{name}", "{a b}"],
    }))
    .expect("a declared tool");
    assert_eq!(tool.placeholders(), [("path", false), ("limit", true)]);
    assert_eq!(tool.package_program(), Some("tools/run.sh"));
}
