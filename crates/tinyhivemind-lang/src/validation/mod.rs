//! Referential integrity, bounded policies and namespace access validation.
//!
//! Every rule pushes its finding, with the file and JSON Pointer it concerns,
//! into one collector. [`validate`] returns the first finding as an
//! [`Error`]; [`diagnose`] returns them all. The rules run in a fixed order,
//! so the first finding is the error a fail-fast check would have stopped
//! at. A check that depends on an earlier one for the same entry, such as
//! the frontmatter of a document that is missing, is skipped rather than
//! reported twice.
mod bounds;
mod integration;
mod memory;
use crate::{
    Diagnostic, Error, Package, Result,
    diagnostic::{Findings, HIVE},
};
use std::collections::{BTreeMap, BTreeSet};

/// The only `$schema` value this version accepts.
const SCHEMA: &str = "https://tinyhumans.ai/schemas/hive/v1";

/// Validate references and host-pinned invariants of a complete candidate.
/// # Errors
/// Returns a typed failure for any malformed identity, reference or bound:
/// the first finding [`diagnose`] would report.
pub fn validate(package: &Package) -> Result<()> {
    let mut found = Findings::default();
    rules(package, &mut found);
    found.into_first()
}

/// Run every validation rule and return every finding, located.
///
/// An empty result means [`validate`] succeeds; otherwise its first element
/// is the error [`validate`] returns.
#[must_use]
pub fn diagnose(package: &Package) -> Vec<Diagnostic> {
    let mut found = Findings::default();
    rules(package, &mut found);
    found.into_diagnostics()
}

/// Push every validation finding for `package`, in fail-fast order.
pub(crate) fn rules(package: &Package, found: &mut Findings) {
    let manifest = &package.manifest;
    if manifest.version != 1 {
        found.push(Error::Schema, HIVE, "/version".to_owned());
    }
    if manifest.schema != SCHEMA {
        found.push(Error::Schema, HIVE, "/$schema".to_owned());
    }
    identities(found, HIVE, "hive", [&manifest.id], |_| "/id".into());
    let roles = identities(
        found,
        HIVE,
        "role",
        manifest.roles.iter().map(|v| &v.id),
        |i| format!("/roles/{i}/id"),
    );
    let seats = identities(
        found,
        HIVE,
        "seat",
        manifest.seats.iter().map(|v| &v.id),
        |i| format!("/seats/{i}/id"),
    );
    let desks = identities(
        found,
        HIVE,
        "desk",
        manifest.desks.iter().map(|v| &v.id),
        |i| format!("/desks/{i}/id"),
    );
    let goals = identities(
        found,
        HIVE,
        "goal",
        manifest.goals.iter().map(|v| &v.id),
        |i| format!("/goals/{i}/id"),
    );
    identities(
        found,
        HIVE,
        "mission",
        manifest.missions.iter().map(|v| &v.id),
        |i| format!("/missions/{i}/id"),
    );
    for path in package.documents.keys() {
        found.check(validate_path(path), path, None);
    }
    roster(package, found, &roles, &seats);
    plan(package, found, &roles, &goals);
    wiring(package, found, &desks);
    bounds::bounds(package, found);
    memory::memory(package, &seats, found);
    integration::integration(package, &seats, found);
}

/// Roles, seats and desk membership.
fn roster(package: &Package, found: &mut Findings, roles: &BTreeSet<&str>, seats: &BTreeSet<&str>) {
    let manifest = &package.manifest;
    for (i, role) in manifest.roles.iter().enumerate() {
        let at = format!("/roles/{i}/charter");
        if declared_document(package, found, &role.charter, "roles/", &at) {
            found.check(charter_frontmatter(package, role), &role.charter, None);
        }
    }
    for (i, seat) in manifest.seats.iter().enumerate() {
        found.check(
            reference("role", &seat.role, roles),
            HIVE,
            format!("/seats/{i}/role"),
        );
        let at = format!("/seats/{i}/prompt");
        let present = declared_document(package, found, &seat.prompt, "seats/", &at);
        if seat.template.trim().is_empty() {
            found.push(
                Error::Identity {
                    kind: "template".into(),
                    id: seat.template.clone(),
                },
                HIVE,
                format!("/seats/{i}/template"),
            );
        }
        if present {
            found.check(frontmatter(package, seat), &seat.prompt, None);
        }
    }
    for (d, desk) in manifest.desks.iter().enumerate() {
        identities(found, HIVE, "desk member", desk.members.iter(), |k| {
            format!("/desks/{d}/members/{k}")
        });
        for (k, member) in desk.members.iter().enumerate() {
            found.check(
                reference("seat", member, seats),
                HIVE,
                format!("/desks/{d}/members/{k}"),
            );
        }
    }
}

/// Goal decomposition and missions.
fn plan(package: &Package, found: &mut Findings, roles: &BTreeSet<&str>, goals: &BTreeSet<&str>) {
    let manifest = &package.manifest;
    for (g, goal) in manifest.goals.iter().enumerate() {
        for (k, child) in goal.subgoals.iter().enumerate() {
            found.check(
                reference("goal", child, goals),
                HIVE,
                format!("/goals/{g}/subgoals/{k}"),
            );
        }
    }
    if let Some(cycle) = goal_cycle(package) {
        let index = manifest.goals.iter().position(|goal| goal.id == cycle);
        found.push(
            Error::GoalCycle { id: cycle },
            HIVE,
            index.map(|i| format!("/goals/{i}")),
        );
    }
    for (m, mission) in manifest.missions.iter().enumerate() {
        found.check(
            reference("role", &mission.role, roles),
            HIVE,
            format!("/missions/{m}/role"),
        );
        for (k, goal) in mission.goals.iter().enumerate() {
            found.check(
                reference("goal", goal, goals),
                HIVE,
                format!("/missions/{m}/goals/{k}"),
            );
        }
    }
}

/// Desk links and shared context documents.
fn wiring(package: &Package, found: &mut Findings, desks: &BTreeSet<&str>) {
    let manifest = &package.manifest;
    for (section, links) in [
        ("topology", &manifest.topology),
        ("referrals", &manifest.referrals),
    ] {
        for (i, link) in links.iter().enumerate() {
            for (end, desk) in [("from", &link.from), ("to", &link.to)] {
                found.check(
                    reference("desk", desk, desks),
                    HIVE,
                    format!("/{section}/{i}/{end}"),
                );
            }
        }
    }
    for (i, path) in manifest.context.iter().enumerate() {
        let at = format!("/context/{i}");
        if declared_document(package, found, path, "context/", &at) {
            found.check(
                crate::patch::validate_context(&package.documents[path])
                    .map_err(|_| Error::Context { path: path.clone() }),
                path,
                None,
            );
        }
    }
}

/// Check identities for blanks and repeats; return the distinct ones seen.
fn identities<'a>(
    found: &mut Findings,
    file: &str,
    kind: &str,
    values: impl IntoIterator<Item = &'a String>,
    at: impl Fn(usize) -> String,
) -> BTreeSet<&'a str> {
    let mut seen = BTreeSet::new();
    for (i, value) in values.into_iter().enumerate() {
        if value.trim().is_empty() || !seen.insert(value.as_str()) {
            found.push(
                Error::Identity {
                    kind: kind.into(),
                    id: value.clone(),
                },
                file,
                at(i),
            );
        }
    }
    seen
}

fn reference(kind: &str, id: &str, known: &BTreeSet<&str>) -> Result<()> {
    if known.contains(id) {
        Ok(())
    } else {
        Err(Error::Reference {
            kind: kind.into(),
            id: id.into(),
        })
    }
}

/// Check that a document key remains a safe relative package path.
/// # Errors
/// Returns [`Error::Path`] for absolute, traversal or control-character paths.
pub fn validate_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path.chars().any(char::is_control)
        || path
            .split('/')
            .any(|v| v.is_empty() || v == "." || v == ".." || v.contains(':'))
    {
        return Err(Error::Path { path: path.into() });
    }
    Ok(())
}

/// Check a manifest-declared document's directory and presence; report
/// against the manifest field `at` that names it.
fn declared_document(
    package: &Package,
    found: &mut Findings,
    path: &str,
    prefix: &str,
    at: &str,
) -> bool {
    found.check(document_purpose(path, prefix), HIVE, at.to_owned())
        && found.check(document(package, path), HIVE, at.to_owned())
}

fn document_purpose(path: &str, prefix: &str) -> Result<()> {
    validate_path(path)?;
    if !path.starts_with(prefix)
        || !std::path::Path::new(path)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
    {
        return Err(Error::Path { path: path.into() });
    }
    Ok(())
}

fn document(package: &Package, path: &str) -> Result<()> {
    if package.documents.contains_key(path) {
        Ok(())
    } else {
        Err(Error::MissingDocument { path: path.into() })
    }
}

/// The first goal found on a decomposition cycle, if any.
fn goal_cycle(package: &Package) -> Option<String> {
    let goals: BTreeMap<_, _> = package
        .manifest
        .goals
        .iter()
        .map(|goal| (goal.id.as_str(), goal))
        .collect();
    let mut done = BTreeSet::new();
    let mut active = BTreeSet::new();
    for id in goals.keys() {
        if done.contains(id) {
            continue;
        }
        let mut stack = vec![(*id, false)];
        while let Some((current, exiting)) = stack.pop() {
            if exiting {
                active.remove(current);
                done.insert(current);
                continue;
            }
            if done.contains(current) {
                continue;
            }
            if !active.insert(current) {
                return Some(current.into());
            }
            stack.push((current, true));
            if let Some(goal) = goals.get(current) {
                for child in goal.subgoals.iter().rev() {
                    stack.push((child.as_str(), false));
                }
            }
        }
    }
    None
}

/// Check namespace syntax independently of a memory engine.
/// # Errors
/// Returns [`Error::Namespace`] for malformed namespace paths.
pub fn validate_namespace(namespace: &str) -> Result<()> {
    let parts: Vec<_> = namespace.split('/').collect();
    if parts.len() > 8
        || parts.iter().any(|part| {
            let Some((kind, id)) = part.split_once(':') else {
                return true;
            };
            !matches!(
                kind,
                "agent" | "team" | "user" | "ws" | "project" | "source"
            ) || id.is_empty()
                || id.len() > 128
                || !id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
    {
        return Err(Error::Namespace {
            namespace: namespace.into(),
        });
    }
    Ok(())
}

/// A role charter's frontmatter may restate only its `id` and `charter`.
fn charter_frontmatter(package: &Package, role: &crate::Role) -> Result<()> {
    let malformed = || Error::Frontmatter {
        seat: role.id.clone(),
    };
    if let Some(rest) = package.documents[&role.charter].strip_prefix("---\n") {
        let (metadata, _) = rest.split_once("\n---\n").ok_or_else(malformed)?;
        let value: serde_json::Value = serde_json::from_str(metadata).map_err(|_| malformed())?;
        let canonical = serde_json::json!({"id":role.id,"charter":role.charter});
        if value.as_object().is_none_or(|fields| {
            fields
                .iter()
                .any(|(key, value)| canonical.get(key) != Some(value))
        }) {
            return Err(malformed());
        }
    }
    Ok(())
}

/// A seat prompt's frontmatter must agree with the seat and its binding.
fn frontmatter(package: &Package, seat: &crate::Seat) -> Result<()> {
    let malformed = || Error::Frontmatter {
        seat: seat.id.clone(),
    };
    let source = &package.documents[&seat.prompt];
    if let Some(rest) = source.strip_prefix("---\n") {
        let (metadata, _) = rest.split_once("\n---\n").ok_or_else(malformed)?;
        let value: serde_json::Value = serde_json::from_str(metadata).map_err(|_| malformed())?;
        let mut canonical = serde_json::to_value(seat).map_err(|_| malformed())?;
        if let Some(binding) = package
            .memory
            .bindings
            .iter()
            .find(|binding| binding.seat == seat.id)
        {
            let settings = serde_json::to_value(binding).map_err(|_| malformed())?;
            for key in ["reads", "recall_at", "budget_chars", "remember"] {
                canonical[key] = settings[key].clone();
            }
            canonical["memory"] = serde_json::Value::String(binding.identity.clone());
        }
        let fields = value.as_object().ok_or_else(malformed)?;
        if fields
            .iter()
            .any(|(key, value)| canonical.get(key) != Some(value))
        {
            return Err(malformed());
        }
    }
    Ok(())
}
#[cfg(test)]
mod test;
#[cfg(test)]
pub(crate) use test::integration::documents as test_documents;
