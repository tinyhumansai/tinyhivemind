//! Resolve a seat's integration: runtime defaults, then its profile, then the
//! seat, with toolpacks expanded and switched-off features removed.
//!
//! Called only on a validated package, so every reference resolves; an id
//! that does not is skipped rather than reported.
use super::{LoweredHook, LoweredIntegrations, LoweredModel, LoweredSeatIntegration};
use crate::{GroupMode, Manifest, Package, Profile, Seat, ToolAccess};
use std::collections::BTreeMap;

/// Whether feature `name` (`mcp`, `skills`, `script_tools` or `hooks`)
/// leaves its integration on: it does unless set to `false`.
fn on(manifest: &Manifest, name: &str) -> bool {
    manifest.features.get(name).copied().unwrap_or(true)
}

/// Append the ids of `more` not already in `list`.
fn extend(list: &mut Vec<String>, more: &[String]) {
    for id in more {
        if !list.contains(id) {
            list.push(id.clone());
        }
    }
}

fn find<'a, T>(items: &'a [T], id: &str, key: impl Fn(&T) -> &str) -> Option<&'a T> {
    items.iter().find(|item| key(item) == id)
}

/// The model `id` with its provider: its own, else the runtime's.
pub(super) fn model(manifest: &Manifest, id: &str) -> Option<LoweredModel> {
    let model = find(&manifest.models, id, |m| &m.id)?.clone();
    let provider = model
        .provider
        .as_ref()
        .or(manifest.runtime.provider.as_ref())
        .and_then(|p| find(&manifest.providers, p, |v| &v.id))
        .cloned();
    Some(LoweredModel { model, provider })
}

/// The hive-wide part of the integration.
pub(super) fn hive(manifest: &Manifest) -> LoweredIntegrations {
    let runtime = &manifest.runtime;
    LoweredIntegrations {
        runtime: runtime.clone(),
        provider: runtime
            .provider
            .as_ref()
            .and_then(|id| find(&manifest.providers, id, |v| &v.id))
            .cloned(),
        model: runtime.model.as_ref().and_then(|id| model(manifest, id)),
        permissions: runtime
            .permissions
            .as_ref()
            .and_then(|id| find(&manifest.permissions, id, |v| &v.id))
            .cloned(),
        skills: if on(manifest, "skills") {
            manifest.skills.clone()
        } else {
            Vec::new()
        },
        session: manifest.session.clone(),
        jev: manifest.jev.clone(),
        features: manifest.features.clone(),
        workflows: manifest.workflows.clone(),
    }
}

/// The ids a seat ends up with before toolpacks expand.
#[derive(Default)]
struct Ids {
    toolpacks: Vec<String>,
    mcp: Vec<String>,
    skills: Vec<String>,
    hooks: Vec<String>,
    context: Vec<String>,
    tools: Vec<String>,
}

/// One seat's resolved integration.
pub(super) fn seat(package: &Package, seat: &Seat) -> LoweredSeatIntegration {
    let m = &package.manifest;
    let profile: Option<&Profile> = seat
        .profile
        .as_ref()
        .and_then(|id| find(&m.profiles, id, |p| &p.id));
    let empty = Profile::default();
    let p = profile.unwrap_or(&empty);

    let mut ids = Ids::default();
    extend(&mut ids.toolpacks, &m.runtime.toolpacks);
    for layer in [
        (&p.toolpacks, &p.mcp, &p.skills, &p.hooks, &p.context),
        (&seat.toolpacks, &seat.mcp, &seat.skills, &seat.hooks, &seat.context),
    ] {
        extend(&mut ids.toolpacks, layer.0);
        extend(&mut ids.mcp, layer.1);
        extend(&mut ids.skills, layer.2);
        extend(&mut ids.hooks, layer.3);
        extend(&mut ids.context, layer.4);
    }
    let mut groups = BTreeMap::<String, GroupMode>::new();
    for id in &ids.toolpacks {
        if let Some(pack) = find(&m.toolpacks, id, |t| &t.id) {
            extend(&mut ids.tools, &pack.tools);
            extend(&mut ids.mcp, &pack.mcp);
            extend(&mut ids.skills, &pack.skills);
            groups.extend(pack.groups.clone());
        }
    }

    let model_id = seat
        .model
        .as_ref()
        .or(p.model.as_ref())
        .or(m.runtime.model.as_ref());
    let model = model_id.and_then(|id| model(m, id));
    let model_limits = model.as_ref().map(|lm| lm.model.limits.clone()).unwrap_or_default();
    let limits = seat
        .limits
        .or(&p.limits)
        .or(&model_limits)
        .or(&m.runtime.limits);

    let permissions = [
        m.runtime.permissions.as_ref(),
        p.permissions.as_ref(),
        seat.permissions.as_ref(),
    ]
    .into_iter()
    .flatten()
    .filter_map(|id| find(&m.permissions, id, |v| &v.id).cloned())
    .collect();

    let mut deny = p.tools.deny.clone();
    extend(&mut deny, &seat.tools.deny);
    let allow = if seat.tools.allow.is_empty() {
        p.tools.allow.clone()
    } else {
        seat.tools.allow.clone()
    };

    let mcp = if on(m, "mcp") {
        resolve(&ids.mcp, &m.mcp, |v| &v.id)
    } else {
        Vec::new()
    };
    let skills = if on(m, "skills") {
        resolve(&ids.skills, &m.skills, |v| &v.id)
    } else {
        Vec::new()
    };
    let programs = if on(m, "script_tools") {
        resolve(&ids.tools, &m.tools, |v| &v.id)
    } else {
        Vec::new()
    };
    let hooks = if on(m, "hooks") {
        ids.hooks
            .iter()
            .filter_map(|id| {
                let hook = find(&m.hooks, id, |h| &h.id)?.clone();
                let tool = find(&m.tools, &hook.run, |t| &t.id)?.clone();
                Some(LoweredHook { hook, tool })
            })
            .collect()
    } else {
        Vec::new()
    };
    let context = ids
        .context
        .iter()
        .filter_map(|path| Some((path.clone(), package.documents.get(path)?.clone())))
        .collect();

    LoweredSeatIntegration {
        profile: profile.map(|p| p.id.clone()),
        agent_id: seat.agent_id.clone(),
        model,
        tools: ToolAccess { allow, deny },
        mcp,
        skills,
        programs,
        hooks,
        groups,
        permissions,
        limits,
        context,
        subagents: p.subagents.clone(),
        bare_prompt: p.bare_prompt,
        definition_base: p.definition_base.clone(),
    }
}

/// The declarations `ids` names, in `ids` order.
fn resolve<T: Clone>(ids: &[String], items: &[T], key: impl Fn(&T) -> &str) -> Vec<T> {
    ids.iter()
        .filter_map(|id| find(items, id, &key).cloned())
        .collect()
}
