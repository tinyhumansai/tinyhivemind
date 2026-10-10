//! Referential integrity, bounded policies and namespace access validation.
use crate::{Error, Package, Result};
use std::collections::{BTreeMap, BTreeSet};

/// Validate references and host-pinned invariants of a complete candidate.
/// # Errors
/// Returns a typed failure for any malformed identity, reference or bound.
pub fn validate(package: &Package) -> Result<()> {
    let manifest = &package.manifest;
    if manifest.version != 1 || manifest.schema != "https://tinyhumans.ai/schemas/hive/v1" {
        return Err(Error::Schema);
    }
    identities("hive", [&manifest.id])?;
    let roles = identities("role", manifest.roles.iter().map(|v| &v.id))?;
    let seats = identities("seat", manifest.seats.iter().map(|v| &v.id))?;
    let desks = identities("desk", manifest.desks.iter().map(|v| &v.id))?;
    let goals = identities("goal", manifest.goals.iter().map(|v| &v.id))?;
    identities("mission", manifest.missions.iter().map(|v| &v.id))?;
    for path in package.documents.keys() {
        validate_path(path)?;
    }
    for role in &manifest.roles {
        document_purpose(&role.charter, "roles/")?;
        document(package, &role.charter)?;
        if let Some(rest) = package.documents[&role.charter].strip_prefix("---\n") {
            let Some((metadata, _)) = rest.split_once("\n---\n") else {
                return Err(Error::Frontmatter {
                    seat: role.id.clone(),
                });
            };
            let value: serde_json::Value =
                serde_json::from_str(metadata).map_err(|_| Error::Frontmatter {
                    seat: role.id.clone(),
                })?;
            let canonical = serde_json::json!({"id":role.id,"charter":role.charter});
            if value.as_object().is_none_or(|fields| {
                fields
                    .iter()
                    .any(|(key, value)| canonical.get(key) != Some(value))
            }) {
                return Err(Error::Frontmatter {
                    seat: role.id.clone(),
                });
            }
        }
    }
    for seat in &manifest.seats {
        reference("role", &seat.role, &roles)?;
        document_purpose(&seat.prompt, "seats/")?;
        document(package, &seat.prompt)?;
        if seat.template.trim().is_empty() {
            return Err(Error::Identity {
                kind: "template".into(),
                id: seat.template.clone(),
            });
        }
        frontmatter(package, seat)?;
    }
    for desk in &manifest.desks {
        identities("desk member", desk.members.iter())?;
        for member in &desk.members {
            reference("seat", member, &seats)?;
        }
    }
    for goal in &manifest.goals {
        for child in &goal.subgoals {
            reference("goal", child, &goals)?;
        }
    }
    validate_goal_graph(package)?;
    for mission in &manifest.missions {
        reference("role", &mission.role, &roles)?;
        for goal in &mission.goals {
            reference("goal", goal, &goals)?;
        }
    }
    for link in manifest.topology.iter().chain(&manifest.referrals) {
        reference("desk", &link.from, &desks)?;
        reference("desk", &link.to, &desks)?;
    }
    for path in &manifest.context {
        document_purpose(path, "context/")?;
        document(package, path)?;
        crate::patch::validate_context(&package.documents[path])
            .map_err(|_| Error::Context { path: path.clone() })?;
    }
    bounds(package)?;
    memory(package, &seats)
}
fn identities<'a>(
    kind: &str,
    values: impl IntoIterator<Item = &'a String>,
) -> Result<BTreeSet<&'a str>> {
    let mut seen = BTreeSet::new();
    for value in values {
        if value.trim().is_empty() || !seen.insert(value.as_str()) {
            return Err(Error::Identity {
                kind: kind.into(),
                id: value.clone(),
            });
        }
    }
    Ok(seen)
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
    validate_path(path)?;
    if package.documents.contains_key(path) {
        Ok(())
    } else {
        Err(Error::MissingDocument { path: path.into() })
    }
}
fn validate_goal_graph(package: &Package) -> Result<()> {
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
                return Err(Error::GoalCycle { id: current.into() });
            }
            stack.push((current, true));
            if let Some(goal) = goals.get(current) {
                for child in goal.subgoals.iter().rev() {
                    stack.push((child.as_str(), false));
                }
            }
        }
    }
    Ok(())
}
fn bound(field: &str, valid: bool) -> Result<()> {
    if valid {
        Ok(())
    } else {
        Err(Error::Bound {
            field: field.into(),
        })
    }
}
fn bounds(package: &Package) -> Result<()> {
    let p = &package.manifest.policies;
    let c = &package.constitution;
    let settings = &package.manifest.coordinator;
    bound("constitution", c.max_turns > 0 && c.max_width > 0)?;
    bound(
        "episode.turn_budget",
        p.episode.turn_budget > 0 && u64::from(p.episode.turn_budget) <= c.max_turns,
    )?;
    bound(
        "episode.width",
        p.episode.round_width > 0
            && p.episode.round_width <= p.episode.revealed_width
            && u64::from(p.episode.revealed_width)
                <= u64::try_from(c.max_width).unwrap_or(u64::MAX),
    )?;
    bound("episode.dominance_cap", p.episode.dominance_cap <= 100)?;
    bound(
        "division.round_width",
        p.division.round_width > 0
            && u64::from(p.division.round_width) <= u64::try_from(c.max_width).unwrap_or(u64::MAX),
    )?;
    bound(
        "routing",
        p.routing.round_width > 0
            && p.routing.round_width <= c.max_width
            && p.routing.choice_option_limit > 0,
    )?;
    bound(
        "routing.probability",
        [
            p.routing.minimum_confidence,
            p.routing.high_impact_minimum_confidence,
            p.routing.clarification_threshold,
            p.routing.high_impact_threshold,
        ]
        .iter()
        .all(|value| value.parts() <= 1_000_000),
    )?;
    bound(
        "coordinator.round_width",
        settings.round_width > 0 && settings.round_width <= c.max_width,
    )?;
    for policy in [&p.conduct, &settings.conduct_policy] {
        bound(
            "conduct",
            policy.child_turn_wall > 0
                && policy.turn_wall > 0
                && policy.child_turn_wall <= policy.turn_wall
                && policy.turn_wall <= c.max_turns,
        )?;
    }
    bound("broadcast_budget", settings.broadcast_budget != Some(0))?;
    bound(
        "digest",
        p.digest.input_limit > 0 && p.digest.budget_chars > 0,
    )?;
    bound(
        "aside",
        !p.aside.enabled || (p.aside.max_members > 0 && p.aside.max_messages > 0),
    )?;
    bound(
        "quorum",
        p.episode.quorum.threshold > 0 && p.episode.quorum.window > 0,
    )?;
    bound("episode.repetition_cap", p.episode.repetition_cap > 0)?;
    bound(
        "half_life",
        p.episode.weights.half_life > 0 && p.directory.half_life > 0,
    )?;
    bound("referral", !p.referral.enabled || p.referral.max_hops > 0)?;
    bound(
        "brevity",
        p.brevity.message_chars > 0 && p.brevity.window > 0,
    )?;
    bound(
        "exchange",
        !p.exchange.enabled || (p.exchange.contact_cap > 0 && p.exchange.round_cap > 0),
    )?;
    Ok(())
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
fn protected(package: &Package, namespace: &str, inherit: bool, descendants: bool) -> Result<()> {
    validate_namespace(namespace)?;
    if package.constitution.protected_namespaces.iter().any(|p| {
        namespace == p
            || namespace
                .strip_prefix(p)
                .is_some_and(|tail| tail.starts_with('/'))
            || (descendants
                && p.strip_prefix(namespace)
                    .is_some_and(|tail| tail.starts_with('/')))
            || (inherit && namespace.starts_with(&format!("{p}/")))
    }) {
        return Err(Error::MemoryAccess {
            namespace: namespace.into(),
        });
    }

    Ok(())
}
fn memory(package: &Package, seats: &BTreeSet<&str>) -> Result<()> {
    let spec = &package.memory;
    validate_namespace(&spec.root)?;
    let ids = identities("memory", spec.identities.iter().map(|v| &v.id))?;
    identities(
        "memory namespace",
        spec.identities.iter().map(|v| &v.namespace),
    )?;
    identities("memory binding", spec.bindings.iter().map(|v| &v.seat))?;
    for namespace in &package.constitution.protected_namespaces {
        validate_namespace(namespace)?;
    }
    for id in &package.constitution.read_only_memories {
        reference("memory", id, &ids)?;
    }
    for identity in &spec.identities {
        protected(package, &identity.namespace, false, false)?;
        if identity.kind == crate::MemoryKind::Agent
            && (!identity
                .namespace
                .rsplit('/')
                .next()
                .is_some_and(|part| part.starts_with("agent:"))
                || identity.namespace == spec.root)
        {
            return Err(Error::MemoryAccess {
                namespace: identity.namespace.clone(),
            });
        }

        if identity.namespace != spec.root
            && !identity.namespace.starts_with(&format!("{}/", spec.root))
        {
            return Err(Error::MemoryAccess {
                namespace: identity.namespace.clone(),
            });
        }
        if let Some(parent) = &identity.fork_parent {
            reference("memory", parent, &ids)?;
            let parent_namespace = &spec
                .identities
                .iter()
                .find(|v| &v.id == parent)
                .ok_or_else(|| Error::Reference {
                    kind: "memory".into(),
                    id: parent.clone(),
                })?
                .namespace;
            if !identity
                .namespace
                .starts_with(&format!("{parent_namespace}/"))
            {
                return Err(Error::MemoryAccess {
                    namespace: identity.namespace.clone(),
                });
            }
        }
        if package
            .constitution
            .read_only_memories
            .contains(&identity.id)
            && !identity.read_only
        {
            return Err(Error::MemoryAccess {
                namespace: identity.namespace.clone(),
            });
        }
    }
    for binding in &spec.bindings {
        reference("seat", &binding.seat, seats)?;
        reference("memory", &binding.identity, &ids)?;
        bound("memory.budget_chars", binding.budget_chars > 0)?;
        for reach in &binding.reads {
            if reach.inherit {
                return Err(Error::MemoryAccess {
                    namespace: reach.at.clone(),
                });
            }
            protected(package, &reach.at, reach.inherit, reach.descendants)?;
            if reach.at != spec.root && !reach.at.starts_with(&format!("{}/", spec.root)) {
                return Err(Error::MemoryAccess {
                    namespace: reach.at.clone(),
                });
            }
        }
    }
    Ok(())
}
fn frontmatter(package: &Package, seat: &crate::Seat) -> Result<()> {
    let source = &package.documents[&seat.prompt];
    if let Some(rest) = source.strip_prefix("---\n") {
        let Some((metadata, _)) = rest.split_once("\n---\n") else {
            return Err(Error::Frontmatter {
                seat: seat.id.clone(),
            });
        };
        let value: serde_json::Value =
            serde_json::from_str(metadata).map_err(|_| Error::Frontmatter {
                seat: seat.id.clone(),
            })?;
        let mut canonical = serde_json::to_value(seat).map_err(|_| Error::Frontmatter {
            seat: seat.id.clone(),
        })?;
        if let Some(binding) = package
            .memory
            .bindings
            .iter()
            .find(|binding| binding.seat == seat.id)
        {
            let settings = serde_json::to_value(binding).map_err(|_| Error::Frontmatter {
                seat: seat.id.clone(),
            })?;
            for key in ["reads", "recall_at", "budget_chars", "remember"] {
                canonical[key] = settings[key].clone();
            }
            canonical["memory"] = serde_json::Value::String(binding.identity.clone());
        }
        let Some(fields) = value.as_object() else {
            return Err(Error::Frontmatter {
                seat: seat.id.clone(),
            });
        };
        if fields
            .iter()
            .any(|(key, value)| canonical.get(key) != Some(value))
        {
            return Err(Error::Frontmatter {
                seat: seat.id.clone(),
            });
        }
    }
    Ok(())
}
#[cfg(test)]
mod test;
