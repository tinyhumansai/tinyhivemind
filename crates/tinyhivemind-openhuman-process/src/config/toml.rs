//! JSON written as TOML, for the shapes a config has.
//!
//! Scalars and lists of scalars are `key = value` lines; tables are `[a.b]`
//! sections and lists of tables `[[a.b]]` sections, after the scalars of
//! their parent. A JSON string is a valid TOML basic string, since the escapes
//! `serde_json` writes (`\"`, `\\`, `\n`, `\uXXXX` and the like) are all TOML
//! escapes, so no value can break out of its line. Keys are the config's own
//! and are bare. `null` is left out, as TOML has none.
use serde_json::{Map, Value};
use std::fmt::Write as _;

/// `tree`, an object, as a TOML document.
pub(super) fn write(tree: &Value) -> String {
    let mut out = String::new();
    if let Value::Object(map) = tree {
        table(&mut out, "", map);
    }
    out
}

fn is_table(value: &Value) -> bool {
    value.is_object()
}

fn is_table_list(value: &Value) -> bool {
    matches!(value, Value::Array(items) if !items.is_empty() && items.iter().all(Value::is_object))
}

fn table(out: &mut String, path: &str, map: &Map<String, Value>) {
    for (key, value) in map {
        if !is_table(value) && !is_table_list(value) && !value.is_null() {
            let _ = writeln!(out, "{key} = {}", inline(value));
        }
    }
    for (key, value) in map {
        let path = if path.is_empty() {
            key.clone()
        } else {
            format!("{path}.{key}")
        };
        match value {
            Value::Object(inner) => {
                let _ = writeln!(out, "[{path}]");
                table(out, &path, inner);
            }
            Value::Array(items) if is_table_list(value) => {
                for item in items.iter().filter_map(Value::as_object) {
                    let _ = writeln!(out, "[[{path}]]");
                    table(out, &path, item);
                }
            }
            _ => {}
        }
    }
}

fn inline(value: &Value) -> String {
    match value {
        Value::Array(items) => {
            let items: Vec<String> = items.iter().map(inline).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Object(map) => {
            let entries = map
                .iter()
                .map(|(key, value)| format!("{key} = {}", inline(value)));
            format!("{{ {} }}", entries.collect::<Vec<_>>().join(", "))
        }
        scalar => scalar.to_string(),
    }
}
