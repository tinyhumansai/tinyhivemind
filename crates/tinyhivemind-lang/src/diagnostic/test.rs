//! The diagnostic wire, rule names, error mapping and the collector.
#![allow(clippy::expect_used)] // Tests fail loudly on unexpected results.
use super::*;
use serde_json::json;

const RULES: [(Rule, &str); 20] = [
    (Rule::Schema, "schema"),
    (Rule::Identity, "identity"),
    (Rule::Reference, "reference"),
    (Rule::Path, "path"),
    (Rule::Bound, "bound"),
    (Rule::GoalCycle, "goal_cycle"),
    (Rule::Namespace, "namespace"),
    (Rule::MemoryAccess, "memory_access"),
    (Rule::Context, "context"),
    (Rule::Frontmatter, "frontmatter"),
    (Rule::Json, "json"),
    (Rule::MissingDocument, "missing_document"),
    (Rule::Integration, "integration"),
    (Rule::ForbiddenClass, "forbidden_class"),
    (Rule::Immutable, "immutable"),
    (Rule::Contamination, "contamination"),
    (Rule::ReadOnly, "read_only"),
    (Rule::Unowned, "unowned"),
    (Rule::Original, "original"),
    (Rule::Sequence, "sequence"),
];

#[test]
fn pins_the_diagnostic_wire_with_and_without_a_pointer() {
    let located = Diagnostic {
        file: "hive.json".into(),
        pointer: Some("/seats/0/role".into()),
        rule: Rule::Reference,
        message: "unresolved role reference ghost".into(),
    };
    assert_eq!(
        serde_json::to_value(&located).expect("serializes"),
        json!({
            "file": "hive.json",
            "pointer": "/seats/0/role",
            "rule": "reference",
            "message": "unresolved role reference ghost"
        })
    );
    let whole = Diagnostic {
        pointer: None,
        file: "seats/a.md".into(),
        ..located.clone()
    };
    let wire = serde_json::to_value(&whole).expect("serializes");
    assert_eq!(wire["pointer"], serde_json::Value::Null);
    assert_eq!(
        serde_json::from_value::<Diagnostic>(wire).expect("decodes"),
        whole
    );
    let unknown = json!({"file": "a", "pointer": null, "rule": "json", "message": "m", "x": 1});
    assert!(serde_json::from_value::<Diagnostic>(unknown).is_err());
}

#[test]
fn displays_compiler_style() {
    let mut diagnostic = Diagnostic {
        file: "hive.json".into(),
        pointer: Some("/seats/3/role".into()),
        rule: Rule::Reference,
        message: "unresolved role reference x".into(),
    };
    assert_eq!(
        diagnostic.to_string(),
        "hive.json:/seats/3/role: reference: unresolved role reference x"
    );
    diagnostic.pointer = None;
    assert_eq!(
        diagnostic.to_string(),
        "hive.json: reference: unresolved role reference x"
    );
}

#[test]
fn every_rule_has_one_snake_case_name_on_the_wire_and_in_display() {
    for (rule, name) in RULES {
        assert_eq!(rule.name(), name);
        assert_eq!(rule.to_string(), name);
        assert_eq!(serde_json::to_value(rule).expect("serializes"), json!(name));
        assert_eq!(
            serde_json::from_value::<Rule>(json!(name)).expect("decodes"),
            rule
        );
    }
}

#[test]
fn every_package_error_maps_to_its_rule() {
    let s = String::new;
    let cases = [
        (Error::MissingDocument { path: s() }, Rule::MissingDocument),
        (
            Error::Json {
                path: s(),
                message: s(),
            },
            Rule::Json,
        ),
        (Error::Schema, Rule::Schema),
        (Error::Identity { kind: s(), id: s() }, Rule::Identity),
        (Error::Reference { kind: s(), id: s() }, Rule::Reference),
        (Error::Path { path: s() }, Rule::Path),
        (Error::Bound { field: s() }, Rule::Bound),
        (Error::GoalCycle { id: s() }, Rule::GoalCycle),
        (Error::Namespace { namespace: s() }, Rule::Namespace),
        (Error::MemoryAccess { namespace: s() }, Rule::MemoryAccess),
        (Error::Context { path: s() }, Rule::Context),
        (Error::Frontmatter { seat: s() }, Rule::Frontmatter),
    ];
    for (error, rule) in cases {
        assert_eq!(Rule::from(&error), rule);
    }
}

#[test]
fn the_collector_keeps_order_drops_repeats_and_yields_the_first_error() {
    let mut found = Findings::default();
    assert!(found.check(Ok(()), HIVE, None));
    assert!(!found.check(Err(Error::Schema), HIVE, "/version".to_owned()));
    found.push(Error::Schema, HIVE, "/version".to_owned());
    found.push(Error::Path { path: "a".into() }, "a", None);
    let diagnostics = Findings(vec![]).into_diagnostics();
    assert_eq!((diagnostics).len(), 0);
    let mut second = Findings::default();
    second.push(Error::Schema, HIVE, None);
    assert_eq!(second.into_first(), Err(Error::Schema));
    assert_eq!(Findings::default().into_first(), Ok(()));
    let diagnostics = found.into_diagnostics();
    assert_eq!(diagnostics.len(), 2);
    assert_eq!(diagnostics[0].pointer.as_deref(), Some("/version"));
    assert_eq!(diagnostics[1].rule, Rule::Path);
}

#[test]
fn escapes_pointer_tokens() {
    assert_eq!(escape("a/b~c"), "a~1b~0c");
    assert_eq!(escape("plain"), "plain");
}
