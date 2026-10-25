//! Check a direct edit of a package's files, like a compiler checks source.
//!
//! An editor changes a hive by rewriting its files. [`check`] compares the
//! original document map with the edited one, runs every language and
//! constitution rule, and collects every violation as a located
//! [`Diagnostic`]. With none, it returns a [`Review`]: the edit split into
//! one [`Patch`] per touched edit class, ready for lineage.
//! [`check_packages`] does the same for two decoded packages.
//!
//! Memory *contents* are not files: learning operations stay on the
//! [`crate::patch::apply`] path.

mod rules;
mod types;
use crate::{
    Diagnostic, Package, Rule, diagnose, diagnose_documents,
    diagnostic::CONSTITUTION,
    patch::{self, CLASSES, Operation, Patch, PatchClass},
};
use std::collections::{BTreeMap, BTreeSet};
pub use types::Review;

/// Check an edited document map against the original it was made from.
///
/// The original must be valid; its findings come back with rule
/// [`Rule::Original`]. The edited map is diagnosed in full, then compared:
/// a changed constitution, protected key or frontmatter; a change of a class
/// the constitution does not permit, or one no class owns; a canary; a
/// change to a read-only memory; and a context edit item deltas cannot
/// express are each reported where they occur, all in one call.
///
/// # Errors
/// Returns every [`Diagnostic`], deduplicated by file, pointer and rule and
/// sorted by file then pointer, when the edit breaks any rule.
pub fn check(
    original: &BTreeMap<String, String>,
    edited: &BTreeMap<String, String>,
) -> Result<Review, Vec<Diagnostic>> {
    let base = valid_original(diagnose_documents(original))?;
    let (edit, mut out) = diagnose_documents(edited);
    rules::outside_package(original, edited, &mut out);
    let operations = if let Some(edit) = &edit {
        compare(&base, edit, &mut out)
    } else {
        undecoded(&base, edited, &mut out);
        vec![]
    };
    // The rules locate manifest findings in `hive.json`; one about a section
    // the edit keeps in a split file belongs in that file.
    for diagnostic in &mut out {
        if let Some(file) =
            crate::document::split::home(&diagnostic.file, diagnostic.pointer.as_deref(), edited)
        {
            file.clone_into(&mut diagnostic.file);
        }
    }
    finish(&base, edit, out, &operations)
}

/// The edit did not decode, so only its raw files can be read: the
/// constitution as JSON, and every other file for canaries.
fn undecoded(base: &Package, edited: &BTreeMap<String, String>, out: &mut Vec<Diagnostic>) {
    if let Some(value) = edited
        .get(CONSTITUTION)
        .and_then(|source| serde_json::from_str(source).ok())
    {
        rules::constitution(&rules::wire(&base.constitution), &value, out);
    }
    let files = edited
        .iter()
        .filter(|(path, _)| path.as_str() != CONSTITUTION)
        .map(|(path, source)| {
            let json = std::path::Path::new(path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("json"));
            let content = serde_json::from_str(source)
                .ok()
                .filter(|_| json)
                .map_or(rules::Content::Text(source), rules::Content::Json);
            (path.as_str(), content)
        });
    rules::canaries(base, files, out);
}

/// Check an edited package against the original it was made from.
///
/// The same rules as [`check`], for a host that already holds decoded
/// packages, such as the whole-package wire form.
///
/// # Errors
/// Returns every [`Diagnostic`] when the edit breaks any rule.
pub fn check_packages(original: &Package, edited: &Package) -> Result<Review, Vec<Diagnostic>> {
    let base = valid_original((Some(original.clone()), diagnose(original)))?;
    let mut out = diagnose(edited);
    let operations = compare(&base, edited, &mut out);
    finish(&base, Some(edited.clone()), out, &operations)
}

/// The original package, or its findings recast as [`Rule::Original`].
fn valid_original(
    (package, diagnostics): (Option<Package>, Vec<Diagnostic>),
) -> Result<Package, Vec<Diagnostic>> {
    match package {
        Some(package) if diagnostics.is_empty() => Ok(package),
        _ => Err(diagnostics
            .into_iter()
            .map(|finding| Diagnostic {
                message: format!(
                    "the original package is invalid: {}: {}",
                    finding.rule, finding.message
                ),
                rule: Rule::Original,
                ..finding
            })
            .collect()),
    }
}

/// Run every edit rule between two decoded packages.
fn compare(
    base: &Package,
    edit: &Package,
    out: &mut Vec<Diagnostic>,
) -> Vec<(PatchClass, Operation)> {
    rules::constitution(
        &rules::wire(&base.constitution),
        &rules::wire(&edit.constitution),
        out,
    );
    rules::protected(base, edit, out);
    rules::canaries(base, rules::package_files(edit), out);
    rules::read_only(base, edit, out);
    rules::classify(base, edit, out)
}

/// Report the findings, or turn the classified operations into a review.
fn finish(
    base: &Package,
    edit: Option<Package>,
    out: Vec<Diagnostic>,
    operations: &[(PatchClass, Operation)],
) -> Result<Review, Vec<Diagnostic>> {
    let mut seen = BTreeSet::new();
    let mut out: Vec<_> = out
        .into_iter()
        .filter(|d| seen.insert((d.file.clone(), d.pointer.clone(), d.rule)))
        .collect();
    out.sort_by(|a, b| (&a.file, &a.pointer).cmp(&(&b.file, &b.pointer)));
    match edit {
        Some(edit) if out.is_empty() => review(base, edit, operations),
        _ => Err(out),
    }
}

/// Group operations into one patch per class and find an order in which
/// every step applies and validates.
fn review(
    base: &Package,
    candidate: Package,
    operations: &[(PatchClass, Operation)],
) -> Result<Review, Vec<Diagnostic>> {
    let patches: Vec<Patch> = CLASSES
        .into_iter()
        .filter_map(|class| {
            let operations: Vec<_> = operations
                .iter()
                .filter(|(owner, _)| *owner == class)
                .map(|(_, operation)| operation.clone())
                .collect();
            (!operations.is_empty()).then_some(Patch { class, operations })
        })
        .collect();
    match order(base, &patches) {
        Some((changes, end)) if end == candidate => Ok(Review { changes, candidate }),
        found => Err(vec![sequence(base, &patches, found.is_some())]),
    }
}

/// The first order of `remaining`, preferring class order, in which every
/// patch applies; and the package it ends at.
fn order(current: &Package, remaining: &[Patch]) -> Option<(Vec<Patch>, Package)> {
    if remaining.is_empty() {
        return Some((vec![], current.clone()));
    }
    remaining.iter().enumerate().find_map(|(i, next)| {
        let applied = patch::apply(current, next).ok()?;
        let mut rest = remaining.to_vec();
        rest.remove(i);
        let (mut tail, end) = order(&applied.candidate, &rest)?;
        tail.insert(0, next.clone());
        Some((tail, end))
    })
}

/// Explain why the classified edit has no valid order of steps.
fn sequence(base: &Package, patches: &[Patch], reproduced: bool) -> Diagnostic {
    let names: Vec<_> = patches
        .iter()
        .filter_map(|patch| rules::wire(&patch.class).as_str().map(str::to_owned))
        .collect();
    let mut current = base.clone();
    let mut failure = (patches.first(), String::new());
    for patch in patches {
        match patch::apply(&current, patch) {
            Ok(applied) => current = applied.candidate,
            Err(error) => {
                failure = (Some(patch), error.to_string());
                break;
            }
        }
    }
    let (failing, error) = failure;
    let file = failing
        .and_then(|patch| patch.operations.first())
        .map_or_else(|| "hive.json".to_owned(), |op| rules::location(op).0);
    let message = if reproduced {
        "the derived changes do not reproduce the edited package".to_owned()
    } else {
        format!(
            "no order of the {} changes is valid at every step ({error}); make the edit as \
             separate smaller edits",
            names.join(", ")
        )
    };
    Diagnostic {
        file,
        pointer: None,
        rule: Rule::Sequence,
        message,
    }
}

#[cfg(test)]
mod test;
