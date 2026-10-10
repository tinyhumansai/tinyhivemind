//! Typed package manifests and caller-supplied document parsing.
mod types;
use crate::{Error, Result};
use std::collections::BTreeMap;
pub use types::*;
/// Parse canonical JSON and Markdown from host-owned strings.
/// # Errors
/// Returns an error for missing documents, malformed JSON or invalid packages.
pub fn parse(documents: &BTreeMap<String, String>) -> Result<Package> {
    for path in documents.keys() {
        crate::validation::validate_path(path)?;
    }
    let package = Package {
        manifest: decode(documents, "hive.json")?,
        constitution: decode(documents, "constitution.json")?,
        memory: decode(documents, "memory.json")?,
        documents: documents
            .iter()
            .filter(|(path, _)| {
                std::path::Path::new(path)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
            })
            .map(|(p, s)| (p.clone(), s.clone()))
            .collect(),
    };
    crate::validate(&package)?;
    Ok(package)
}
fn decode<T: serde::de::DeserializeOwned + serde::Serialize>(
    documents: &BTreeMap<String, String>,
    path: &str,
) -> Result<T> {
    let source = documents
        .get(path)
        .ok_or_else(|| Error::MissingDocument { path: path.into() })?;
    let input: serde_json::Value = serde_json::from_str(source).map_err(|error| Error::Json {
        path: path.into(),
        message: error.to_string(),
    })?;
    let decoded: T = serde_json::from_value(input.clone()).map_err(|error| Error::Json {
        path: path.into(),
        message: error.to_string(),
    })?;
    let output = serde_json::to_value(&decoded).map_err(|error| Error::Json {
        path: path.into(),
        message: error.to_string(),
    })?;
    if has_unknown_fields(&input, &output) {
        return Err(Error::Json {
            path: path.into(),
            message: "unknown nested schema field".into(),
        });
    }
    Ok(decoded)
}
pub(crate) fn has_unknown_fields(input: &serde_json::Value, output: &serde_json::Value) -> bool {
    match (input, output) {
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            a.iter().any(|(key, value)| {
                if key == "responder_mode" && value == "lead" && b.contains_key("members") {
                    return false;
                }
                b.get(key)
                    .is_none_or(|current| has_unknown_fields(value, current))
            })
        }
        (serde_json::Value::Array(a), serde_json::Value::Array(b)) => {
            a.iter().zip(b).any(|(a, b)| has_unknown_fields(a, b))
        }
        _ => false,
    }
}
