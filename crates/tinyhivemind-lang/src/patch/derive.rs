//! Derive operations from two package states, and name the class owning each.
//!
//! [`super::diff`] and [`crate::check()`] share these: the operations for one
//! changed manifest or memory field, one created or retired document, and one
//! changed document body or context item.

use super::{Operation, PatchClass, Result, boundary, items};
use serde_json::Value;

/// Every edit class, in the order derived changes are recorded.
pub(crate) const CLASSES: [PatchClass; 8] = [
    PatchClass::Prompt,
    PatchClass::Context,
    PatchClass::Memory,
    PatchClass::Roster,
    PatchClass::Topology,
    PatchClass::Policy,
    PatchClass::Goal,
    PatchClass::Integration,
];

/// The class whose boundary admits `operation`, if any class does.
pub(crate) fn owner(operation: &Operation) -> Option<PatchClass> {
    CLASSES
        .into_iter()
        .find(|class| boundary(*class, operation))
}

/// One `json` operation per differing top-level field of `root` (`manifest`
/// or `memory`) between two whole-package wire values.
pub(crate) fn fields(old: &Value, new: &Value, root: &str) -> Vec<Operation> {
    let empty = serde_json::Map::new();
    let old_fields = old[root].as_object().unwrap_or(&empty);
    let new_fields = new[root].as_object().unwrap_or(&empty);
    let keys: std::collections::BTreeSet<_> = old_fields.keys().chain(new_fields.keys()).collect();
    keys.into_iter()
        .filter(|key| old_fields.get(*key) != new_fields.get(*key))
        .map(|key| Operation::Json {
            pointer: format!("/{root}/{key}"),
            before: old_fields.get(key).cloned(),
            after: new_fields.get(key).cloned(),
        })
        .collect()
}

/// The `document` operation creating or retiring `path`, if exactly one side
/// holds it.
pub(crate) fn lifecycle(
    path: &str,
    before: Option<&String>,
    after: Option<&String>,
) -> Option<Operation> {
    (before.is_some() != after.is_some()).then(|| Operation::Document {
        path: path.to_owned(),
        before: before.cloned(),
        after: after.cloned(),
    })
}

/// The operations rewriting an existing document from `before` to `after`:
/// one `context` operation per changed item under `context/`, otherwise one
/// `prompt` operation for the body.
///
/// # Errors
/// Rejects malformed context markers and a changed frontmatter
/// ([`super::Error::Immutable`]).
pub(crate) fn modification(path: &str, before: &str, after: &str) -> Result<Vec<Operation>> {
    if before == after {
        return Ok(vec![]);
    }
    if super::whole(path) {
        return Ok(vec![Operation::Prompt {
            path: path.to_owned(),
            before: before.into(),
            after: after.into(),
        }]);
    }
    if path.starts_with("context/") {
        let old_items = items::items(before)?;
        let new_items = items::items(after)?;
        // Document order: existing items first, then appended ones as written.
        let mut keys: Vec<&String> = Vec::new();
        for (id, _, _, _) in old_items.iter().chain(&new_items) {
            if !keys.contains(&id) {
                keys.push(id);
            }
        }
        let body = |list: &[(String, usize, usize, String)], id: &String| {
            list.iter()
                .find(|(key, _, _, _)| key == id)
                .map(|(_, _, _, body)| body.clone())
        };
        return Ok(keys
            .into_iter()
            .filter_map(|id| {
                let (before, after) = (body(&old_items, id), body(&new_items, id));
                (before != after).then(|| Operation::Context {
                    path: path.to_owned(),
                    item: id.clone(),
                    before,
                    after,
                })
            })
            .collect());
    }
    let old_offset = items::body_offset(before);
    let new_offset = items::body_offset(after);
    if before[..old_offset] != after[..new_offset] {
        return Err(super::Error::Immutable);
    }
    Ok(vec![Operation::Prompt {
        path: path.to_owned(),
        before: before[old_offset..].into(),
        after: after[new_offset..].into(),
    }])
}

/// Replay `operations` (context item deltas) on `document`.
///
/// # Errors
/// Rejects an item edit [`super::apply`] would also reject.
pub(crate) fn replay_items(document: &str, operations: &[&Operation]) -> Result<String> {
    let mut document = document.to_owned();
    for operation in operations {
        if let Operation::Context {
            item,
            before,
            after,
            ..
        } = operation
        {
            document = items::edit(&document, item, before.as_deref(), after.as_deref())?;
        }
    }
    Ok(document)
}
