//! The ledger's turn rules and bounds, and the argument schema checker.
use super::ledger::{Identity, Known, Ledger, Outcome};
use super::schema::{check, unsupported};
use serde_json::json;

fn id(turn: &str, call: &str) -> Identity {
    Identity {
        seat: "a".into(),
        turn: turn.into(),
        call: call.into(),
    }
}
fn running(turn: &'static str) -> impl Fn() -> Option<String> {
    move || Some(turn.to_owned())
}

#[test]
fn a_stale_start_neither_records_nor_replaces_the_running_turns_records() {
    let ledger = Ledger::new(8, 4096);
    let args = json!({});
    assert_eq!(
        ledger.start(&id("t2", "c"), "x", &args, running("t2")),
        Known::New
    );
    ledger.finish(&id("t2", "c"), Outcome::success("done"));
    assert_eq!(
        ledger.start(&id("t1", "c"), "x", &args, running("t2")),
        Known::Stale
    );
    assert_eq!(ledger.lookup(&id("t1", "c"), "x", &args), Known::New);
    assert_eq!(
        ledger.lookup(&id("t2", "c"), "x", &args),
        Known::Answered(Outcome::success("done"))
    );
}

#[test]
fn abandon_forget_and_purge_drop_records() {
    let ledger = Ledger::new(8, 4096);
    let args = json!({});
    ledger.start(&id("t1", "c"), "x", &args, running("t1"));
    assert_eq!(ledger.lookup(&id("t1", "c"), "x", &args), Known::Running);
    ledger.abandon(&id("t1", "c"));
    assert_eq!(ledger.lookup(&id("t1", "c"), "x", &args), Known::New);
    ledger.start(&id("t1", "c"), "x", &args, running("t1"));
    ledger.purge(|_| Some("t1".into()));
    assert_eq!(ledger.lookup(&id("t1", "c"), "x", &args), Known::Running);
    ledger.purge(|_| None);
    assert_eq!(ledger.lookup(&id("t1", "c"), "x", &args), Known::New);
    ledger.start(&id("t1", "c"), "x", &args, running("t1"));
    ledger.forget("a");
    assert_eq!(ledger.lookup(&id("t1", "c"), "x", &args), Known::New);
}

#[test]
fn a_full_seat_refuses_and_a_newer_turn_starts_empty() {
    let ledger = Ledger::new(1, 4096);
    let args = json!({});
    assert_eq!(
        ledger.start(&id("t1", "a"), "x", &args, running("t1")),
        Known::New
    );
    assert_eq!(
        ledger.start(&id("t1", "b"), "x", &args, running("t1")),
        Known::Full
    );
    assert_eq!(
        ledger.start(&id("t1", "a"), "y", &args, running("t1")),
        Known::Reused
    );
    assert_eq!(
        ledger.start(&id("t2", "b"), "x", &args, running("t2")),
        Known::New
    );
    let tight = Ledger::new(8, 8);
    let big = json!({"note": "far too long for the bound"});
    assert_eq!(
        tight.start(&id("t1", "a"), "x", &big, running("t1")),
        Known::Full
    );
}

#[test]
fn the_schema_checker_enforces_every_keyword_it_accepts() {
    let schema = json!({
        "type": "object",
        "description": "fixture",
        "properties": {
            "name": {"type": "string", "minLength": 2, "maxLength": 4},
            "count": {"type": "integer", "minimum": 1, "maximum": 3},
            "ratio": {"type": "number", "exclusiveMinimum": 0, "exclusiveMaximum": 1},
            "mode": {"enum": ["a", "b"]},
            "fixed": {"const": 7},
            "tags": {"type": "array", "items": {"type": "string"}, "minItems": 1, "maxItems": 2},
            "maybe": {"type": ["string", "null"]},
            "flag": {"type": "boolean"},
            "nested": {"type": "object", "required": ["x"], "additionalProperties": {"type": "integer"}}
        },
        "additionalProperties": false
    });
    assert_eq!(unsupported(&schema), None);
    let fits = json!({
        "name": "abc", "count": 2, "ratio": 0.5, "mode": "a", "fixed": 7,
        "tags": ["t"], "maybe": null, "flag": true, "nested": {"x": 1, "y": 2}
    });
    assert_eq!(check(&schema, &fits, ""), Ok(()));
    for (field, value, said) in [
        ("name", json!("a"), "name has the wrong length"),
        ("count", json!(9), "count is out of range"),
        ("count", json!(1.5), "count must be integer"),
        ("ratio", json!(1), "ratio is out of range"),
        ("mode", json!("c"), "mode is not one of the allowed values"),
        ("fixed", json!(8), "fixed is not the allowed value"),
        ("tags", json!([]), "tags has the wrong length"),
        ("tags", json!([1]), "tags[0] must be string"),
        ("maybe", json!(1), "maybe must be string or null"),
        ("flag", json!("yes"), "flag must be boolean"),
        ("nested", json!({"y": 1}), "missing argument nested.x"),
        (
            "nested",
            json!({"x": 1, "y": "z"}),
            "nested.y must be integer",
        ),
        ("other", json!(1), "unknown argument other"),
    ] {
        let mut value_set = fits.clone();
        value_set[field] = value;
        assert_eq!(
            check(&schema, &value_set, ""),
            Err(said.to_owned()),
            "{field}"
        );
    }
    assert_eq!(check(&json!(true), &json!(1), ""), Ok(()));
}

#[test]
fn an_unchecked_keyword_is_found_at_any_depth() {
    assert_eq!(
        unsupported(&json!({"type": "object", "oneOf": []})),
        Some("oneOf".into())
    );
    let nested = json!({"properties": {"a": {"items": {"pattern": "x"}}}});
    assert_eq!(unsupported(&nested), Some("pattern".into()));
    let extra = json!({"additionalProperties": {"format": "uri", "$ref": "#"}});
    assert_eq!(unsupported(&extra), Some("$ref".into()));
    assert_eq!(unsupported(&json!(true)), None);
}
