//! Atomic editor proposals, stable Markdown deltas, and immutable host guards.

mod items;
mod types;
use crate::Package;
use serde_json::Value;
pub use types::{AppliedPatch, Error, Operation, Patch, PatchClass, Result};

pub(crate) fn validate_context(source: &str) -> Result<()> {
    items::items(source).map(|_| ())
}

fn boundary(class: PatchClass, op: &Operation) -> bool {
    match op {
        Operation::Document {
            path,
            before,
            after,
        } => {
            before.is_some() != after.is_some()
                && std::path::Path::new(path)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
                && match class {
                    PatchClass::Roster => path.starts_with("seats/") || path.starts_with("roles/"),
                    PatchClass::Context => path.starts_with("context/"),
                    _ => false,
                }
        }
        Operation::Prompt { path, .. } => {
            class == PatchClass::Prompt
                && (path.starts_with("seats/") || path.starts_with("roles/"))
                && std::path::Path::new(path)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        }
        Operation::Context { path, .. } => {
            class == PatchClass::Context
                && path.starts_with("context/")
                && std::path::Path::new(path)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        }
        Operation::Learning { .. } => class == PatchClass::Memory,
        Operation::Json { pointer, .. } => {
            let section = pointer.split('/').nth(2).unwrap_or_default();
            if pointer.starts_with("/memory/") {
                return class == PatchClass::Memory;
            }
            pointer.starts_with("/manifest/")
                && match class {
                    PatchClass::Roster => matches!(section, "roles" | "seats" | "desks"),
                    PatchClass::Topology => matches!(section, "topology" | "referrals"),
                    PatchClass::Policy => matches!(section, "policies" | "coordinator"),
                    PatchClass::Goal => matches!(section, "goals" | "missions"),
                    PatchClass::Context => section == "context",
                    _ => false,
                }
        }
    }
}

fn protected(value: &Value) -> std::collections::BTreeMap<String, Value> {
    fn visit(
        value: &Value,
        path: &str,
        found: &mut std::collections::BTreeMap<String, Value>,
        goals: bool,
    ) {
        match value {
            Value::Object(map) => {
                for (key, value) in map {
                    let next = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                    if matches!(
                        key.as_str(),
                        "evaluation" | "telemetry" | "logging" | "constitution"
                    ) || (key == "acceptance" && !goals)
                    {
                        found.insert(next, value.clone());
                    } else {
                        visit(
                            value,
                            &next,
                            found,
                            goals || (key == "goals" && path.is_empty()),
                        );
                    }
                }
            }
            Value::Array(values) => {
                for (index, value) in values.iter().enumerate() {
                    visit(value, &format!("{path}/{index}"), found, goals);
                }
            }
            _ => {}
        }
    }
    let mut found = std::collections::BTreeMap::new();
    visit(value, "", &mut found, false);
    found
}

fn protected_configuration(package: &Package) -> Result<std::collections::BTreeMap<String, Value>> {
    let mut values = protected(&serde_json::to_value(&package.manifest)?);
    for (path, source) in &package.documents {
        if let Some(tail) = source.strip_prefix("---\n") {
            let Some((metadata, _)) = tail.split_once("\n---\n") else {
                return Err(Error::Immutable);
            };
            let metadata: Value = serde_json::from_str(metadata)?;
            for (pointer, value) in protected(&metadata) {
                values.insert(
                    format!(
                        "/documents/{}/frontmatter{pointer}",
                        path.replace('~', "~0").replace('/', "~1")
                    ),
                    value,
                );
            }
        }
    }
    Ok(values)
}

fn contains_canary(value: &Value, canary: &str) -> bool {
    match value {
        Value::String(text) => text.contains(canary),
        Value::Array(values) => values.iter().any(|value| contains_canary(value, canary)),
        Value::Object(values) => values
            .iter()
            .any(|(key, value)| key.contains(canary) || contains_canary(value, canary)),
        _ => false,
    }
}

fn mutate(
    value: &mut Value,
    pointer: &str,
    before: Option<&Value>,
    after: Option<&Value>,
) -> Result<()> {
    let (parent_path, token) = pointer
        .rsplit_once('/')
        .ok_or_else(|| Error::Pointer(pointer.into()))?;
    if token.is_empty()
        || token
            .as_bytes()
            .windows(2)
            .any(|pair| pair[0] == b'~' && pair[1] != b'0' && pair[1] != b'1')
        || token.ends_with('~')
    {
        return Err(Error::Pointer(pointer.into()));
    }
    let key = token.replace("~1", "/").replace("~0", "~");
    let parent = value
        .pointer_mut(parent_path)
        .ok_or_else(|| Error::Pointer(pointer.into()))?;
    match parent {
        Value::Object(map) => {
            if map.get(&key) != before {
                return Err(Error::Stale(pointer.into()));
            }
            if let Some(after) = after {
                map.insert(key, after.clone());
            } else {
                map.remove(&key);
            }
        }
        Value::Array(values) => {
            let index: usize = key.parse().map_err(|_| Error::Pointer(pointer.into()))?;
            if key != index.to_string() || index > values.len() {
                return Err(Error::Pointer(pointer.into()));
            }
            if values.get(index) != before {
                return Err(Error::Stale(pointer.into()));
            }
            match after {
                Some(after) if index == values.len() => values.push(after.clone()),
                Some(after) => values[index] = after.clone(),
                None if index < values.len() => {
                    values.remove(index);
                }
                None => {}
            }
        }
        _ => return Err(Error::Pointer(pointer.into())),
    }
    Ok(())
}

fn candidate(package: &Package, patch: &Patch) -> Result<AppliedPatch> {
    let mut wire = serde_json::to_value(package)?;
    let mut documents = package.documents.clone();
    let mut learning = Vec::new();
    let mut document_lifecycles = std::collections::BTreeSet::new();
    for operation in &patch.operations {
        if !boundary(patch.class, operation) {
            return Err(Error::ClassBoundary(match operation {
                Operation::Json { pointer, .. } => pointer.clone(),
                Operation::Document { path, .. }
                | Operation::Prompt { path, .. }
                | Operation::Context { path, .. } => path.clone(),
                Operation::Learning { identity, .. } => identity.clone(),
            }));
        }
        match operation {
            Operation::Json {
                pointer,
                before,
                after,
            } => mutate(&mut wire, pointer, before.as_ref(), after.as_ref())?,
            Operation::Document {
                path,
                before,
                after,
            } => {
                if !document_lifecycles.insert(path.clone()) {
                    return Err(Error::ClassBoundary(path.clone()));
                }
                if documents.get(path) != before.as_ref() {
                    return Err(Error::Stale(path.clone()));
                }
                if path.starts_with("context/")
                    && let Some(after) = after
                {
                    items::items(after)?;
                }
                if let Some(after) = after {
                    documents.insert(path.clone(), after.clone());
                } else {
                    documents.remove(path);
                }
            }
            Operation::Prompt {
                path,
                before,
                after,
            } => {
                let document = documents
                    .get_mut(path)
                    .ok_or_else(|| Error::Stale(path.clone()))?;
                let offset = items::body_offset(document);
                if &document[offset..] != before {
                    return Err(Error::Stale(path.clone()));
                }
                if after.starts_with("---\n") {
                    return Err(Error::Immutable);
                }
                document.replace_range(offset.., after);
            }
            Operation::Context {
                path,
                item,
                before,
                after,
            } => {
                let document = documents
                    .get_mut(path)
                    .ok_or_else(|| Error::Stale(path.clone()))?;
                *document = items::edit(document, item, before.as_deref(), after.as_deref())?;
            }
            Operation::Learning { .. } => learning.push(operation.clone()),
        }
    }
    let mut candidate: Package = serde_json::from_value(wire.clone())?;
    if crate::document::has_unknown_fields(&wire, &serde_json::to_value(&candidate)?) {
        return Err(Error::UnknownField);
    }
    candidate.documents = documents;
    Ok(AppliedPatch {
        candidate,
        learning,
    })
}

/// Apply a proposal atomically and guard the full resulting package.
///
/// # Errors
/// Rejects stale preconditions, class violations, contamination, immutable changes,
/// read-only learning, and any invalid candidate. No caller-owned value is mutated.
pub fn apply(package: &Package, patch: &Patch) -> Result<AppliedPatch> {
    let applied = candidate(package, patch)?;
    guard(package, &applied.candidate, patch)?;
    Ok(applied)
}

/// Verify that a complete candidate is exactly the proposal and is permitted.
///
/// # Errors
/// Rejects a forged candidate, forbidden class, immutable settings, read-only
/// memory changes, canaries, or invalid package invariants and memory reaches.
pub fn guard(original: &Package, proposed: &Package, patch: &Patch) -> Result<()> {
    if !original.constitution.permitted_edits.contains(&patch.class) {
        return Err(Error::ForbiddenClass);
    }
    if original.constitution != proposed.constitution {
        return Err(Error::Immutable);
    }
    if protected_configuration(original)? != protected_configuration(proposed)? {
        return Err(Error::Immutable);
    }
    if candidate(original, patch)?.candidate != *proposed {
        return Err(Error::CandidateMismatch);
    }
    let serialized = serde_json::json!({"manifest": proposed.manifest, "memory": proposed.memory, "documents": proposed.documents});
    let patch_wire = serde_json::to_value(patch)?;
    if original
        .constitution
        .contamination_canaries
        .iter()
        .any(|canary| {
            !canary.is_empty()
                && (contains_canary(&serialized, canary) || contains_canary(&patch_wire, canary))
        })
    {
        return Err(Error::Contamination);
    }
    for id in &original.constitution.read_only_memories {
        let old_identity = original
            .memory
            .identities
            .iter()
            .find(|identity| &identity.id == id);
        let new_identity = proposed
            .memory
            .identities
            .iter()
            .find(|identity| &identity.id == id);
        let old_bindings: Vec<_> = original
            .memory
            .bindings
            .iter()
            .filter(|binding| &binding.identity == id)
            .collect();
        let new_bindings: Vec<_> = proposed
            .memory
            .bindings
            .iter()
            .filter(|binding| &binding.identity == id)
            .collect();
        if old_identity != new_identity || old_bindings != new_bindings {
            return Err(Error::ReadOnly(id.clone()));
        }
    }
    for operation in &patch.operations {
        if let Operation::Learning {
            identity,
            entry,
            before,
            after,
            ..
        } = operation
        {
            let memory = proposed
                .memory
                .identities
                .iter()
                .find(|memory| &memory.id == identity)
                .ok_or_else(|| Error::Learning(identity.clone()))?;
            if entry.is_empty() || (before.is_none() && after.is_none()) {
                return Err(Error::Learning(entry.clone()));
            }
            if memory.read_only || original.constitution.read_only_memories.contains(identity) {
                return Err(Error::ReadOnly(identity.clone()));
            }
        }
    }
    crate::validate(proposed)?;
    Ok(())
}

/// Compute one class of edits between packages without discarding context items.
///
/// # Errors
/// Rejects differences outside the chosen class, immutable settings, malformed
/// context markers, or unrepresentable document addition/removal and preambles.
pub fn diff(original: &Package, proposed: &Package, class: PatchClass) -> Result<Patch> {
    let old = serde_json::to_value(original)?;
    let new = serde_json::to_value(proposed)?;
    let mut operations = Vec::new();
    for root in ["manifest", "memory"] {
        let old_fields = old[root]
            .as_object()
            .ok_or_else(|| Error::Pointer(root.into()))?;
        let new_fields = new[root]
            .as_object()
            .ok_or_else(|| Error::Pointer(root.into()))?;
        let keys: std::collections::BTreeSet<_> =
            old_fields.keys().chain(new_fields.keys()).collect();
        for key in keys {
            if old_fields.get(key) != new_fields.get(key) {
                operations.push(Operation::Json {
                    pointer: format!("/{root}/{key}"),
                    before: old_fields.get(key).cloned(),
                    after: new_fields.get(key).cloned(),
                });
            }
        }
    }
    let paths: std::collections::BTreeSet<_> = original
        .documents
        .keys()
        .chain(proposed.documents.keys())
        .collect();
    for path in paths {
        if original.documents.contains_key(path) != proposed.documents.contains_key(path) {
            operations.push(Operation::Document {
                path: path.clone(),
                before: original.documents.get(path).cloned(),
                after: proposed.documents.get(path).cloned(),
            });
        }
    }
    for (path, before) in &original.documents {
        let Some(after) = proposed.documents.get(path) else {
            continue;
        };
        if before == after {
            continue;
        }
        if path.starts_with("context/") {
            let old_items = items::items(before)?;
            let new_items = items::items(after)?;
            let keys: std::collections::BTreeSet<_> = old_items
                .iter()
                .chain(&new_items)
                .map(|(id, _, _, _)| id)
                .collect();
            for id in keys {
                let before = old_items
                    .iter()
                    .find(|(key, _, _, _)| key == id)
                    .map(|(_, _, _, body)| body.clone());
                let after = new_items
                    .iter()
                    .find(|(key, _, _, _)| key == id)
                    .map(|(_, _, _, body)| body.clone());
                if before != after {
                    operations.push(Operation::Context {
                        path: path.clone(),
                        item: id.clone(),
                        before,
                        after,
                    });
                }
            }
        } else {
            let old_offset = items::body_offset(before);
            let new_offset = items::body_offset(after);
            if before[..old_offset] != after[..new_offset] {
                return Err(Error::Immutable);
            }
            operations.push(Operation::Prompt {
                path: path.clone(),
                before: before[old_offset..].into(),
                after: after[new_offset..].into(),
            });
        }
    }
    let patch = Patch { class, operations };
    guard(original, proposed, &patch)?;
    Ok(patch)
}

#[cfg(test)]
mod test;
