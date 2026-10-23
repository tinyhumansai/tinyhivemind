//! The edit rules: each compares the original and edited package and pushes
//! a located diagnostic for every violation it finds.

use crate::{
    Diagnostic, Package, Rule, SeatMemory,
    diagnostic::{CONSTITUTION, HIVE, MEMORY, escape},
    patch::{self, Operation, PatchClass},
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// A package value as wire JSON. The wire types serialize infallibly; a
/// failure would surface as `null` and so as a difference, never as a pass.
pub(super) fn wire<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn diagnostic(file: &str, pointer: Option<String>, rule: Rule, message: String) -> Diagnostic {
    Diagnostic {
        file: file.to_owned(),
        pointer,
        rule,
        message,
    }
}

/// The pointer suffix of the first leaf at which `before` and `after` differ.
pub(super) fn first_difference(before: Option<&Value>, after: Option<&Value>) -> String {
    match (before, after) {
        (Some(Value::Object(a)), Some(Value::Object(b))) => a
            .keys()
            .chain(b.keys())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .find(|key| a.get(*key) != b.get(*key))
            .map(|key| {
                format!(
                    "/{}{}",
                    escape(key),
                    first_difference(a.get(key), b.get(key))
                )
            })
            .unwrap_or_default(),
        (Some(Value::Array(a)), Some(Value::Array(b))) => (0..a.len().max(b.len()))
            .find(|i| a.get(*i) != b.get(*i))
            .map(|i| format!("/{i}{}", first_difference(a.get(i), b.get(i))))
            .unwrap_or_default(),
        _ => String::new(),
    }
}

/// Where a derived operation lives: its file and, for JSON, its first
/// differing leaf.
pub(super) fn location(operation: &Operation) -> (String, Option<String>) {
    match operation {
        Operation::Json {
            pointer,
            before,
            after,
        } => {
            let (file, field) = match pointer.strip_prefix("/manifest") {
                Some(field) => (HIVE, field),
                None => (MEMORY, pointer.strip_prefix("/memory").unwrap_or(pointer)),
            };
            let leaf = first_difference(before.as_ref(), after.as_ref());
            (file.to_owned(), Some(format!("{field}{leaf}")))
        }
        Operation::Document { path, .. }
        | Operation::Prompt { path, .. }
        | Operation::Context { path, .. } => (path.clone(), None),
        Operation::Learning { .. } => (MEMORY.to_owned(), None),
    }
}

/// Report every changed top-level field of the constitution.
pub(super) fn constitution(old: &Value, new: &Value, out: &mut Vec<Diagnostic>) {
    let message = || "the constitution is host-owned and no edit may change it".to_owned();
    match (old, new) {
        (Value::Object(a), Value::Object(b)) => {
            for key in a.keys().chain(b.keys()).collect::<BTreeSet<_>>() {
                if a.get(key) != b.get(key) {
                    let leaf = first_difference(a.get(key), b.get(key));
                    let pointer = format!("/{}{leaf}", escape(key));
                    out.push(diagnostic(
                        CONSTITUTION,
                        Some(pointer),
                        Rule::Immutable,
                        message(),
                    ));
                }
            }
        }
        _ if old != new => out.push(diagnostic(CONSTITUTION, None, Rule::Immutable, message())),
        _ => {}
    }
}

/// Protected keys of the manifest and of every parseable frontmatter.
fn protected_entries(package: &Package) -> BTreeMap<(String, String), Value> {
    let mut entries: BTreeMap<_, _> = patch::protected(&wire(&package.manifest))
        .into_iter()
        .map(|(pointer, value)| ((HIVE.to_owned(), pointer), value))
        .collect();
    for (path, source) in &package.documents {
        let Some((metadata, _)) = source
            .strip_prefix("---\n")
            .and_then(|tail| tail.split_once("\n---\n"))
        else {
            continue;
        };
        if let Ok(metadata) = serde_json::from_str::<Value>(metadata) {
            for (pointer, value) in patch::protected(&metadata) {
                entries.insert((path.clone(), pointer), value);
            }
        }
    }
    entries
}

/// Report every protected key added, removed or changed.
pub(super) fn protected(base: &Package, edit: &Package, out: &mut Vec<Diagnostic>) {
    let old = protected_entries(base);
    let new = protected_entries(edit);
    for key in old.keys().chain(new.keys()).collect::<BTreeSet<_>>() {
        if old.get(key) == new.get(key) {
            continue;
        }
        let (file, pointer) = key;
        let token = pointer.rsplit('/').next().unwrap_or_default();
        let message = format!("{token} settings are host-owned and no edit may change them");
        // A frontmatter pointer is not a pointer into the Markdown file, so
        // it travels in the message instead.
        let (pointer, message) = if file == HIVE {
            (Some(pointer.clone()), message)
        } else {
            (None, format!("{message} (frontmatter {pointer})"))
        };
        out.push(diagnostic(file, pointer, Rule::Immutable, message));
    }
}

/// File contents a canary scan reads: decoded JSON, or raw text.
pub(super) enum Content<'a> {
    /// A decoded JSON file, scanned so a hit can name its pointer.
    Json(Value),
    /// A Markdown or undecodable file.
    Text(&'a str),
}

/// The pointer of the first string or key in `value` containing `canary`.
fn find_canary(value: &Value, canary: &str) -> Option<String> {
    match value {
        Value::String(text) => text.contains(canary).then(String::new),
        Value::Array(values) => values
            .iter()
            .enumerate()
            .find_map(|(i, value)| find_canary(value, canary).map(|tail| format!("/{i}{tail}"))),
        Value::Object(map) => map.iter().find_map(|(key, value)| {
            if key.contains(canary) {
                Some(format!("/{}", escape(key)))
            } else {
                find_canary(value, canary).map(|tail| format!("/{}{tail}", escape(key)))
            }
        }),
        _ => None,
    }
}

/// Report every file that contains one of the original constitution's
/// canaries.
pub(super) fn canaries<'a>(
    base: &Package,
    files: impl IntoIterator<Item = (&'a str, Content<'a>)>,
    out: &mut Vec<Diagnostic>,
) {
    let canaries: Vec<_> = base
        .constitution
        .contamination_canaries
        .iter()
        .filter(|canary| !canary.is_empty())
        .collect();
    for (file, content) in files {
        let hit = canaries.iter().find_map(|canary| match &content {
            Content::Json(value) => find_canary(value, canary).map(Some),
            Content::Text(text) => text.contains(canary.as_str()).then_some(None),
        });
        if let Some(pointer) = hit {
            out.push(diagnostic(
                file,
                pointer,
                Rule::Contamination,
                "contains a contamination canary the constitution holds out".into(),
            ));
        }
    }
}

/// The scan inputs for a decoded package: everything but the constitution.
pub(super) fn package_files(package: &Package) -> Vec<(&str, Content<'_>)> {
    let mut files = vec![
        (HIVE, Content::Json(wire(&package.manifest))),
        (MEMORY, Content::Json(wire(&package.memory))),
    ];
    files.extend(
        package
            .documents
            .iter()
            .map(|(path, source)| (path.as_str(), Content::Text(source))),
    );
    files
}

/// The bindings of memory `id`, with their indices.
fn bindings<'a>(package: &'a Package, id: &str) -> Vec<(usize, &'a SeatMemory)> {
    package
        .memory
        .bindings
        .iter()
        .enumerate()
        .filter(|(_, binding)| binding.identity == id)
        .collect()
}

/// Report a changed identity, or changed bindings, of a pinned memory.
pub(super) fn read_only(base: &Package, edit: &Package, out: &mut Vec<Diagnostic>) {
    for id in &base.constitution.read_only_memories {
        let message = || format!("memory {id} is pinned read-only by the constitution");
        let find = |package: &Package| {
            package
                .memory
                .identities
                .iter()
                .position(|identity| &identity.id == id)
        };
        let (old, new) = (find(base), find(edit));
        if old.map(|i| &base.memory.identities[i]) != new.map(|i| &edit.memory.identities[i]) {
            let pointer = new.or(old).map(|i| format!("/identities/{i}"));
            out.push(diagnostic(MEMORY, pointer, Rule::ReadOnly, message()));
        }
        let (old, new) = (bindings(base, id), bindings(edit, id));
        if !old.iter().map(|(_, b)| b).eq(new.iter().map(|(_, b)| b)) {
            let index = new.first().or(old.first()).map(|(i, _)| *i);
            let pointer = index.map(|i| format!("/bindings/{i}"));
            out.push(diagnostic(MEMORY, pointer, Rule::ReadOnly, message()));
        }
    }
}

/// Check that a context document's edit is expressible as item deltas.
fn context_replay(path: &str, before: &str, after: &str, ops: &[Operation]) -> Option<Diagnostic> {
    let ops: Vec<_> = ops.iter().collect();
    let message = match patch::replay_items(before, &ops) {
        Err(error) => error.to_string(),
        Ok(replayed) if replayed != after => "text outside the stable items changed, or items \
             moved; edit inside items and add new items after the last one"
            .into(),
        Ok(_) => return None,
    };
    Some(diagnostic(path, None, Rule::Context, message))
}

/// Derive every operation between the packages, report the ones that are
/// unowned, forbidden or unrepresentable, and return the rest by class.
pub(super) fn classify(
    base: &Package,
    edit: &Package,
    out: &mut Vec<Diagnostic>,
) -> Vec<(PatchClass, Operation)> {
    let (old, new) = (wire(base), wire(edit));
    let mut operations = patch::fields(&old, &new, "manifest");
    operations.extend(patch::fields(&old, &new, "memory"));
    let paths: BTreeSet<_> = base.documents.keys().chain(edit.documents.keys()).collect();
    for path in paths {
        let (before, after) = (base.documents.get(path), edit.documents.get(path));
        operations.extend(patch::lifecycle(path, before, after));
        if before.is_none()
            && let Some(after) = after
            && path.starts_with("context/")
            && let Err(error) = patch::validate_context(after)
        {
            out.push(diagnostic(path, None, Rule::Context, error.to_string()));
        }
        let (Some(before), Some(after)) = (before, after) else {
            continue;
        };
        match patch::modification(path, before, after) {
            Ok(list) => {
                if path.starts_with("context/") {
                    out.extend(context_replay(path, before, after, &list));
                }
                if !patch::whole(path)
                    && list.iter().any(|operation| {
                        matches!(operation, Operation::Prompt { after, .. } if after.starts_with("---\n"))
                    })
                {
                    out.push(diagnostic(
                        path,
                        None,
                        Rule::Immutable,
                        "the body may not open a second frontmatter block".into(),
                    ));
                }
                operations.extend(list);
            }
            Err(patch::Error::Immutable) => out.push(diagnostic(
                path,
                None,
                Rule::Immutable,
                "frontmatter restates the manifest and may not be edited; change the body only"
                    .into(),
            )),
            Err(error) => out.push(diagnostic(path, None, Rule::Context, error.to_string())),
        }
    }
    let permitted = &base.constitution.permitted_edits;
    let mut owned = Vec::new();
    for operation in operations {
        let (file, pointer) = location(&operation);
        match patch::owner(&operation) {
            None => {
                let what = pointer.as_deref().map_or(file.as_str(), |pointer| {
                    pointer.split('/').nth(1).unwrap_or_default()
                });
                let message = format!(
                    "no edit class owns {what}; only seats/, roles/, context/, tools/ and \
                     skills/ documents and the manifest and memory sections a class names may \
                     change"
                );
                out.push(diagnostic(&file, pointer, Rule::Unowned, message));
            }
            Some(class) if !permitted.contains(&class) => {
                let name = wire(&class);
                let message = format!(
                    "the constitution does not permit {} edits",
                    name.as_str().unwrap_or_default()
                );
                out.push(diagnostic(&file, pointer, Rule::ForbiddenClass, message));
            }
            Some(class) => owned.push((class, operation)),
        }
    }
    owned
}

/// Report every changed file that is not part of a package at all.
pub(super) fn outside_package(
    original: &BTreeMap<String, String>,
    edited: &BTreeMap<String, String>,
    out: &mut Vec<Diagnostic>,
) {
    let keys: BTreeSet<_> = original.keys().chain(edited.keys()).collect();
    for key in keys {
        if crate::document::carried(key) || [HIVE, CONSTITUTION, MEMORY].contains(&key.as_str()) {
            continue;
        }
        if original.get(key) != edited.get(key) {
            out.push(diagnostic(
                key,
                None,
                Rule::Unowned,
                "not part of the package; only hive.json, constitution.json, memory.json, \
                 markdown documents and files under tools/ and skills/ are read"
                    .into(),
            ));
        }
    }
}
