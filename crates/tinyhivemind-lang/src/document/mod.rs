//! Typed package manifests and caller-supplied document parsing.
//!
//! [`parse`] and [`diagnose_documents`] share one collector: every unsafe
//! path, every canonical file that is missing or does not decode, and, once
//! all three decode, every validation finding. [`parse`] returns the first;
//! [`diagnose_documents`] returns them all.
mod defaults;
mod integration;
mod types;
use crate::{
    Diagnostic, Error, Result,
    diagnostic::{CONSTITUTION, Findings, HIVE, MEMORY, escape},
};
use std::collections::BTreeMap;
pub use integration::*;
pub use types::*;

/// Parse canonical JSON and Markdown from host-owned strings.
/// # Errors
/// Returns an error for missing documents, malformed JSON or invalid packages:
/// the first finding [`diagnose_documents`] would report.
pub fn parse(documents: &BTreeMap<String, String>) -> Result<Package> {
    let (package, found) = collect(documents);
    found.into_first()?;
    // `collect` withholds the package only after recording why, so with no
    // finding the package is present; this arm keeps the function total.
    package.ok_or_else(|| Error::MissingDocument {
        path: HIVE.to_owned(),
    })
}

/// Decode and validate a document map, reporting every finding at once.
///
/// The package is `Some` whenever `hive.json`, `constitution.json` and
/// `memory.json` decode, whether or not it then validates; it is valid
/// exactly when the diagnostics are empty. A decode failure carries serde's
/// line and column in its message, and an unknown nested field is reported
/// at its JSON Pointer.
#[must_use]
pub fn diagnose_documents(
    documents: &BTreeMap<String, String>,
) -> (Option<Package>, Vec<Diagnostic>) {
    let (package, found) = collect(documents);
    (package, found.into_diagnostics())
}

/// Every finding for `documents`, in the order fail-fast parsing meets them.
pub(crate) fn collect(documents: &BTreeMap<String, String>) -> (Option<Package>, Findings) {
    let mut found = Findings::default();
    for path in documents.keys() {
        found.check(crate::validation::validate_path(path), path, None);
    }
    let manifest = decode(documents, HIVE, &mut found);
    let constitution = decode(documents, CONSTITUTION, &mut found);
    let memory = decode(documents, MEMORY, &mut found);
    let (Some(manifest), Some(constitution), Some(memory)) = (manifest, constitution, memory)
    else {
        return (None, found);
    };
    let package = Package {
        manifest,
        constitution,
        memory,
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
    crate::validation::rules(&package, &mut found);
    (Some(package), found)
}

fn decode<T: serde::de::DeserializeOwned + serde::Serialize>(
    documents: &BTreeMap<String, String>,
    path: &str,
    found: &mut Findings,
) -> Option<T> {
    let json = |message: String| Error::Json {
        path: path.into(),
        message,
    };
    let Some(source) = documents.get(path) else {
        found.push(Error::MissingDocument { path: path.into() }, path, None);
        return None;
    };
    let input: serde_json::Value = match serde_json::from_str(source) {
        Ok(input) => input,
        Err(error) => {
            found.push(json(error.to_string()), path, None);
            return None;
        }
    };
    let decoded: T = match serde_json::from_value(input.clone()) {
        Ok(decoded) => decoded,
        Err(error) => {
            // Decoding the source text again locates the schema error by
            // line and column, which a decoded value has forgotten.
            let located = serde_json::from_str::<T>(source).err().unwrap_or(error);
            found.push(json(located.to_string()), path, None);
            return None;
        }
    };
    // The wire types serialize infallibly; were one ever to fail, a null
    // output simply skips the unknown-field comparison.
    let output = serde_json::to_value(&decoded).unwrap_or(serde_json::Value::Null);
    if let Some(pointer) = positional(&input, &output) {
        let pointer = Some(pointer).filter(|pointer| !pointer.is_empty());
        found.push(json("expected a JSON object".into()), path, pointer);
        return None;
    }
    if let Some(pointer) = unknown_field(&input, &output) {
        found.push(json("unknown nested schema field".into()), path, pointer);
        return None;
    }
    Some(decoded)
}

/// The JSON Pointer of the first array the typed output holds as an object.
///
/// serde decodes a struct from a JSON array of its fields in order, and a
/// struct whose fields all have defaults from an empty one, so without this
/// pass `"tools": []` or a `constitution.json` of `[]` would quietly decode
/// as the defaults. Every section of the language is written as an object.
fn positional(input: &serde_json::Value, output: &serde_json::Value) -> Option<String> {
    match (input, output) {
        (serde_json::Value::Array(_), serde_json::Value::Object(_)) => Some(String::new()),
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            a.iter().find_map(|(key, value)| {
                let tail = positional(value, b.get(key)?)?;
                Some(format!("/{}{tail}", escape(key)))
            })
        }
        (serde_json::Value::Array(a), serde_json::Value::Array(b)) => a
            .iter()
            .zip(b)
            .enumerate()
            .find_map(|(i, (a, b))| positional(a, b).map(|tail| format!("/{i}{tail}"))),
        _ => None,
    }
}

pub(crate) fn has_unknown_fields(input: &serde_json::Value, output: &serde_json::Value) -> bool {
    unknown_field(input, output).is_some()
}

/// The JSON Pointer of the first input key the typed output dropped.
fn unknown_field(input: &serde_json::Value, output: &serde_json::Value) -> Option<String> {
    match (input, output) {
        (serde_json::Value::Object(a), serde_json::Value::Object(b)) => {
            a.iter().find_map(|(key, value)| {
                if key == "responder_mode" && value == "lead" && b.contains_key("members") {
                    return None;
                }
                let at = format!("/{}", escape(key));
                match b.get(key) {
                    None => Some(at),
                    Some(current) => unknown_field(value, current).map(|tail| at + &tail),
                }
            })
        }
        (serde_json::Value::Array(a), serde_json::Value::Array(b)) => a
            .iter()
            .zip(b)
            .enumerate()
            .find_map(|(i, (a, b))| unknown_field(a, b).map(|tail| format!("/{i}{tail}"))),
        _ => None,
    }
}

#[cfg(test)]
mod test;
