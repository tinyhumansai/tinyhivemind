//! Arguments checked against an executable tool's declared schema.
//!
//! The language only requires `args` to be an object schema, and no JSON
//! Schema validator is linked outside the harness, so this checks the subset a
//! tool argument schema uses: `type` (a name or a list of names), `enum`,
//! `const`, `properties`, `required`, `additionalProperties` (a boolean or a
//! schema), `items`, `minimum`, `maximum`, `exclusiveMinimum`,
//! `exclusiveMaximum`, `minLength`, `maxLength`, `minItems` and `maxItems`.
//! Annotations (`description`, `title`, `default`, `examples`, `format`,
//! `$comment`) check nothing. A schema using any other keyword is refused
//! when its seat is served, so no keyword is ever accepted and left
//! unchecked.
use serde_json::{Map, Value};

const CHECKED: [&str; 15] = [
    "type",
    "enum",
    "const",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "minimum",
    "maximum",
    "exclusiveMinimum",
    "exclusiveMaximum",
    "minLength",
    "maxLength",
    "minItems",
    "maxItems",
];
const ANNOTATIONS: [&str; 6] = [
    "description",
    "title",
    "default",
    "examples",
    "format",
    "$comment",
];

/// The first keyword in `schema`, at any depth, that is not checked here.
pub(crate) fn unsupported(schema: &Value) -> Option<String> {
    let object = schema.as_object()?;
    for (keyword, value) in object {
        let known = CHECKED.contains(&keyword.as_str()) || ANNOTATIONS.contains(&keyword.as_str());
        if !known {
            return Some(keyword.clone());
        }
        let nested = match keyword.as_str() {
            "properties" => value
                .as_object()
                .and_then(|properties| properties.values().find_map(unsupported)),
            "items" | "additionalProperties" => unsupported(value),
            _ => None,
        };
        if nested.is_some() {
            return nested;
        }
    }
    None
}

/// Check `value` against `schema`, naming the first place it does not fit.
pub(crate) fn check(schema: &Value, value: &Value, at: &str) -> Result<(), String> {
    let Some(schema) = schema.as_object() else {
        return Ok(());
    };
    if let Some(kinds) = schema.get("type") {
        let names: Vec<&str> = match kinds {
            Value::String(name) => vec![name.as_str()],
            Value::Array(names) => names.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        if !names.iter().any(|name| is(name, value)) {
            return Err(format!("{at} must be {}", names.join(" or ")));
        }
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array)
        && !allowed.contains(value)
    {
        return Err(format!("{at} is not one of the allowed values"));
    }
    if schema
        .get("const")
        .is_some_and(|constant| constant != value)
    {
        return Err(format!("{at} is not the allowed value"));
    }
    bounds(schema, value, at)?;
    if let (Some(items), Some(values)) = (schema.get("items"), value.as_array()) {
        for (index, item) in values.iter().enumerate() {
            check(items, item, &format!("{at}[{index}]"))?;
        }
    }
    if let Some(object) = value.as_object() {
        fields(schema, object, at)?;
    }
    Ok(())
}

fn is(name: &str, value: &Value) -> bool {
    match name {
        "string" => value.is_string(),
        "integer" => {
            value.is_i64() || value.is_u64() || value.as_f64().is_some_and(|n| n.fract() == 0.0)
        }
        "number" => value.is_number(),
        "boolean" => value.is_boolean(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        "null" => value.is_null(),
        _ => false,
    }
}

fn bounds(schema: &Map<String, Value>, value: &Value, at: &str) -> Result<(), String> {
    let limit = |keyword: &str| schema.get(keyword).and_then(Value::as_f64);
    let count = |keyword: &str| schema.get(keyword).and_then(Value::as_u64);
    if let Some(number) = value.as_f64() {
        let fails = limit("minimum").is_some_and(|min| number < min)
            || limit("maximum").is_some_and(|max| number > max)
            || limit("exclusiveMinimum").is_some_and(|min| number <= min)
            || limit("exclusiveMaximum").is_some_and(|max| number >= max);
        if fails {
            return Err(format!("{at} is out of range"));
        }
    }
    let length = match value {
        Value::String(text) => Some((text.chars().count() as u64, "Length")),
        Value::Array(items) => Some((items.len() as u64, "Items")),
        _ => None,
    };
    if let Some((length, unit)) = length {
        let fails = count(&format!("min{unit}")).is_some_and(|min| length < min)
            || count(&format!("max{unit}")).is_some_and(|max| length > max);
        if fails {
            return Err(format!("{at} has the wrong length"));
        }
    }
    Ok(())
}

fn fields(
    schema: &Map<String, Value>,
    object: &Map<String, Value>,
    at: &str,
) -> Result<(), String> {
    let path = |name: &str| {
        if at.is_empty() {
            name.to_owned()
        } else {
            format!("{at}.{name}")
        }
    };
    let properties = schema.get("properties").and_then(Value::as_object);
    for required in schema
        .get("required")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        if !object.contains_key(required) {
            return Err(format!("missing argument {}", path(required)));
        }
    }
    for (name, field) in object {
        match properties.and_then(|properties| properties.get(name)) {
            Some(declared) => check(declared, field, &path(name))?,
            None => match schema.get("additionalProperties") {
                Some(Value::Bool(false)) => {
                    return Err(format!("unknown argument {}", path(name)));
                }
                Some(extra @ Value::Object(_)) => check(extra, field, &path(name))?,
                _ => {}
            },
        }
    }
    Ok(())
}
