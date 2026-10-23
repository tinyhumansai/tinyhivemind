//! Integration sections: identities, references, shipped files, bounds and
//! the settings the harness cannot run as written.
//!
//! Every reference resolves inside the package. Bounds report
//! [`Error::Bound`] naming the setting; an unrunnable shape reports
//! [`Error::Integration`].
use super::{declared_document, identities, reference, validate_path};
use crate::{
    Error, ExecTool, HookEvent, Limits, Manifest, Package, SecretRef, WorkflowTarget,
    diagnostic::{Findings, HIVE},
};
use std::collections::{BTreeMap, BTreeSet};

/// The ids each integration section declares.
struct Declared<'a> {
    providers: BTreeSet<&'a str>,
    models: BTreeSet<&'a str>,
    mcp: BTreeSet<&'a str>,
    skills: BTreeSet<&'a str>,
    tools: BTreeSet<&'a str>,
    toolpacks: BTreeSet<&'a str>,
    permissions: BTreeSet<&'a str>,
    profiles: BTreeSet<&'a str>,
    hooks: BTreeSet<&'a str>,
}

/// Push every integration finding for `package`.
pub(super) fn integration(package: &Package, seats: &BTreeSet<&str>, found: &mut Findings) {
    let m = &package.manifest;
    let declared = Declared {
        providers: section(
            found,
            "provider",
            "providers",
            m.providers.iter().map(|v| &v.id),
        ),
        models: section(found, "model", "models", m.models.iter().map(|v| &v.id)),
        mcp: section(found, "mcp server", "mcp", m.mcp.iter().map(|v| &v.id)),
        skills: section(found, "skill", "skills", m.skills.iter().map(|v| &v.id)),
        tools: section(found, "tool", "tools", m.tools.iter().map(|v| &v.id)),
        toolpacks: section(
            found,
            "toolpack",
            "toolpacks",
            m.toolpacks.iter().map(|v| &v.id),
        ),
        permissions: section(
            found,
            "permission set",
            "permissions",
            m.permissions.iter().map(|v| &v.id),
        ),
        profiles: section(
            found,
            "profile",
            "profiles",
            m.profiles.iter().map(|v| &v.id),
        ),
        hooks: section(found, "hook", "hooks", m.hooks.iter().map(|v| &v.id)),
    };
    section(
        found,
        "workflow",
        "workflows",
        m.workflows.iter().map(|v| &v.id),
    );
    references(package, &declared, seats, found);
    shipped(package, found);
    bounds(m, found);
    shapes(m, found);
}

fn section<'a>(
    found: &mut Findings,
    kind: &str,
    name: &str,
    ids: impl IntoIterator<Item = &'a String>,
) -> BTreeSet<&'a str> {
    identities(found, HIVE, kind, ids, |i| format!("/{name}/{i}/id"))
}

/// Check one optional reference at `at`.
fn one(found: &mut Findings, kind: &str, id: Option<&String>, known: &BTreeSet<&str>, at: String) {
    if let Some(id) = id {
        found.check(reference(kind, id, known), HIVE, at);
    }
}

/// Check every reference of a list at `at/<k>`.
fn all(found: &mut Findings, kind: &str, ids: &[String], known: &BTreeSet<&str>, at: &str) {
    for (k, id) in ids.iter().enumerate() {
        found.check(reference(kind, id, known), HIVE, format!("{at}/{k}"));
    }
}

/// The references one seat or profile makes, with `at` its JSON Pointer.
struct Uses<'a> {
    at: String,
    model: Option<&'a String>,
    permissions: Option<&'a String>,
    toolpacks: &'a [String],
    mcp: &'a [String],
    skills: &'a [String],
    hooks: &'a [String],
    context: &'a [String],
}

fn uses(package: &Package, d: &Declared<'_>, u: &Uses<'_>, found: &mut Findings) {
    let at = &u.at;
    one(found, "model", u.model, &d.models, format!("{at}/model"));
    one(
        found,
        "permission set",
        u.permissions,
        &d.permissions,
        format!("{at}/permissions"),
    );
    all(
        found,
        "toolpack",
        u.toolpacks,
        &d.toolpacks,
        &format!("{at}/toolpacks"),
    );
    all(found, "mcp server", u.mcp, &d.mcp, &format!("{at}/mcp"));
    all(found, "skill", u.skills, &d.skills, &format!("{at}/skills"));
    all(found, "hook", u.hooks, &d.hooks, &format!("{at}/hooks"));
    for (k, path) in u.context.iter().enumerate() {
        declared_document(
            package,
            found,
            path,
            "context/",
            &format!("{at}/context/{k}"),
        );
    }
}

fn references(package: &Package, d: &Declared<'_>, seats: &BTreeSet<&str>, found: &mut Findings) {
    let m = &package.manifest;
    let r = &m.runtime;
    one(
        found,
        "provider",
        r.provider.as_ref(),
        &d.providers,
        "/runtime/provider".into(),
    );
    one(
        found,
        "model",
        r.model.as_ref(),
        &d.models,
        "/runtime/model".into(),
    );
    one(
        found,
        "permission set",
        r.permissions.as_ref(),
        &d.permissions,
        "/runtime/permissions".into(),
    );
    all(
        found,
        "toolpack",
        &r.toolpacks,
        &d.toolpacks,
        "/runtime/toolpacks",
    );
    for (i, model) in m.models.iter().enumerate() {
        one(
            found,
            "provider",
            model.provider.as_ref(),
            &d.providers,
            format!("/models/{i}/provider"),
        );
    }
    for (i, pack) in m.toolpacks.iter().enumerate() {
        all(
            found,
            "tool",
            &pack.tools,
            &d.tools,
            &format!("/toolpacks/{i}/tools"),
        );
        all(
            found,
            "mcp server",
            &pack.mcp,
            &d.mcp,
            &format!("/toolpacks/{i}/mcp"),
        );
        all(
            found,
            "skill",
            &pack.skills,
            &d.skills,
            &format!("/toolpacks/{i}/skills"),
        );
    }
    for (i, hook) in m.hooks.iter().enumerate() {
        found.check(
            reference("tool", &hook.run, &d.tools),
            HIVE,
            format!("/hooks/{i}/run"),
        );
    }
    agents(package, d, seats, found);
}

/// References made by profiles, seats and workflows.
fn agents(package: &Package, d: &Declared<'_>, seats: &BTreeSet<&str>, found: &mut Findings) {
    let m = &package.manifest;
    for (i, p) in m.profiles.iter().enumerate() {
        let at = format!("/profiles/{i}");
        all(
            found,
            "profile",
            &p.subagents,
            &d.profiles,
            &format!("{at}/subagents"),
        );
        let u = Uses {
            at,
            model: p.model.as_ref(),
            permissions: p.permissions.as_ref(),
            toolpacks: &p.toolpacks,
            mcp: &p.mcp,
            skills: &p.skills,
            hooks: &p.hooks,
            context: &p.context,
        };
        uses(package, d, &u, found);
    }
    for (i, s) in m.seats.iter().enumerate() {
        let at = format!("/seats/{i}");
        one(
            found,
            "profile",
            s.profile.as_ref(),
            &d.profiles,
            format!("{at}/profile"),
        );
        let u = Uses {
            at,
            model: s.model.as_ref(),
            permissions: s.permissions.as_ref(),
            toolpacks: &s.toolpacks,
            mcp: &s.mcp,
            skills: &s.skills,
            hooks: &s.hooks,
            context: &s.context,
        };
        uses(package, d, &u, found);
    }
    for (i, job) in m.workflows.iter().enumerate() {
        let at = format!("/workflows/{i}/target");
        match &job.target {
            WorkflowTarget::Seat(seat) => {
                found.check(reference("seat", seat, seats), HIVE, at);
            }
            WorkflowTarget::Hive(hive) => {
                let this = BTreeSet::from([m.id.as_str()]);
                found.check(reference("hive", hive, &this), HIVE, at);
            }
        }
    }
}

/// Skill bundles and package programs must ship in the package.
fn shipped(package: &Package, found: &mut Findings) {
    let m = &package.manifest;
    for (i, skill) in m.skills.iter().enumerate() {
        let at = format!("/skills/{i}/id");
        let manifest = format!("skills/{}/SKILL.md", skill.id);
        if skill.id.contains('/') || validate_path(&manifest).is_err() {
            found.push(Error::Path { path: manifest }, HIVE, at);
        } else if !package.documents.contains_key(&manifest) {
            found.push(Error::MissingDocument { path: manifest }, HIVE, at);
        }
    }
    for (i, tool) in m.tools.iter().enumerate() {
        if let Some(program) = tool.package_program() {
            let at = format!("/tools/{i}/command/0");
            if found.check(validate_path(program), HIVE, at.clone())
                && !package.documents.contains_key(program)
            {
                found.push(
                    Error::MissingDocument {
                        path: program.into(),
                    },
                    HIVE,
                    at,
                );
            }
        }
    }
}

fn bound(found: &mut Findings, ok: bool, field: String, at: String) {
    if !ok {
        found.push(Error::Bound { field }, HIVE, at);
    }
}

fn limits(found: &mut Findings, limits: &Limits, field: &str, at: &str) {
    bound(
        found,
        limits.timeout_ms != Some(0),
        format!("{field}.limits.timeout_ms"),
        format!("{at}/limits/timeout_ms"),
    );
    bound(
        found,
        limits.iterations != Some(0),
        format!("{field}.limits.iterations"),
        format!("{at}/limits/iterations"),
    );
    bound(
        found,
        limits
            .budget_usd
            .is_none_or(|usd| usd.is_finite() && usd >= 0.0),
        format!("{field}.limits.budget_usd"),
        format!("{at}/limits/budget_usd"),
    );
}

fn bounds(m: &Manifest, found: &mut Findings) {
    let r = &m.runtime;
    bound(
        found,
        r.concurrency != 0,
        "runtime.concurrency".into(),
        "/runtime/concurrency".into(),
    );
    bound(
        found,
        r.max_agents != Some(0),
        "runtime.max_agents".into(),
        "/runtime/max_agents".into(),
    );
    limits(found, &r.limits, "runtime", "/runtime");
    for (i, model) in m.models.iter().enumerate() {
        let at = format!("/models/{i}");
        bound(
            found,
            model.temperature.is_none_or(|t| t.is_finite() && t >= 0.0),
            format!("models.{}.temperature", model.id),
            format!("{at}/temperature"),
        );
        bound(
            found,
            model.max_tokens != Some(0),
            format!("models.{}.max_tokens", model.id),
            format!("{at}/max_tokens"),
        );
        limits(found, &model.limits, &format!("models.{}", model.id), &at);
    }
    for (i, p) in m.profiles.iter().enumerate() {
        limits(
            found,
            &p.limits,
            &format!("profiles.{}", p.id),
            &format!("/profiles/{i}"),
        );
    }
    for (i, s) in m.seats.iter().enumerate() {
        limits(
            found,
            &s.limits,
            &format!("seats.{}", s.id),
            &format!("/seats/{i}"),
        );
    }
    for (i, tool) in m.tools.iter().enumerate() {
        bound(
            found,
            tool.timeout_ms != 0,
            format!("tools.{}.timeout_ms", tool.id),
            format!("/tools/{i}/timeout_ms"),
        );
    }
    for (i, hook) in m.hooks.iter().enumerate() {
        bound(
            found,
            hook.timeout_ms != Some(0),
            format!("hooks.{}.timeout_ms", hook.id),
            format!("/hooks/{i}/timeout_ms"),
        );
    }
    let s = &m.session;
    for (value, key) in [
        (s.max_history_messages, "max_history_messages"),
        (s.tool_result_budget_bytes, "tool_result_budget_bytes"),
        (s.max_tool_iterations, "max_tool_iterations"),
    ] {
        bound(
            found,
            value != Some(0),
            format!("session.{key}"),
            format!("/session/{key}"),
        );
    }
    bound(
        found,
        m.jev.top_k != 0,
        "jev.top_k".into(),
        "/jev/top_k".into(),
    );
    for (i, job) in m.workflows.iter().enumerate() {
        bound(
            found,
            job.schedule.split_whitespace().count() == 5,
            format!("workflows.{}.schedule", job.id),
            format!("/workflows/{i}/schedule"),
        );
    }
}

fn invalid(found: &mut Findings, field: String, reason: &str, at: String) {
    found.push(
        Error::Integration {
            field,
            reason: reason.into(),
        },
        HIVE,
        at,
    );
}

fn name_ok(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn secrets(found: &mut Findings, values: &BTreeMap<String, SecretRef>, field: &str, at: &str) {
    for (key, value) in values {
        let (SecretRef::Env(name) | SecretRef::Store(name)) = value;
        if name.trim().is_empty() {
            let token = crate::diagnostic::escape(key);
            invalid(
                found,
                format!("{field}.{key}"),
                "blank secret reference",
                format!("{at}/{token}"),
            );
        }
    }
}

fn shapes(m: &Manifest, found: &mut Findings) {
    for (i, p) in m.providers.iter().enumerate() {
        if let Some(SecretRef::Env(name) | SecretRef::Store(name)) = &p.credential
            && name.trim().is_empty()
        {
            invalid(
                found,
                format!("providers.{}.credential", p.id),
                "blank secret reference",
                format!("/providers/{i}/credential"),
            );
        }
    }
    for (i, server) in m.mcp.iter().enumerate() {
        let at = format!("/mcp/{i}");
        if server.endpoint.trim().is_empty() {
            invalid(
                found,
                format!("mcp.{}.endpoint", server.id),
                "empty endpoint",
                format!("{at}/endpoint"),
            );
        }
        secrets(
            found,
            &server.env,
            &format!("mcp.{}.env", server.id),
            &format!("{at}/env"),
        );
        secrets(
            found,
            &server.headers,
            &format!("mcp.{}.headers", server.id),
            &format!("{at}/headers"),
        );
    }
    for (i, set) in m.permissions.iter().enumerate() {
        let allowed = set.allow_tools.iter().flatten().collect::<BTreeSet<_>>();
        if let Some(k) = set
            .deny_tools
            .iter()
            .position(|tool| allowed.contains(tool))
        {
            invalid(
                found,
                format!("permissions.{}.deny_tools", set.id),
                "tool both allowed and denied",
                format!("/permissions/{i}/deny_tools/{k}"),
            );
        }
    }
    for (i, tool) in m.tools.iter().enumerate() {
        tool_shape(tool, i, found);
    }
    for (i, hook) in m.hooks.iter().enumerate() {
        if hook.event == HookEvent::Stop && hook.matcher.is_some() {
            invalid(
                found,
                format!("hooks.{}.matcher", hook.id),
                "a stop hook has no tool to match",
                format!("/hooks/{i}/matcher"),
            );
        }
    }
    for name in m.features.keys() {
        if !name_ok(name) {
            let token = crate::diagnostic::escape(name);
            invalid(
                found,
                format!("features.{name}"),
                "feature names are lowercase letters, digits and underscores",
                format!("/features/{token}"),
            );
        }
    }
}

fn tool_shape(tool: &ExecTool, i: usize, found: &mut Findings) {
    let at = format!("/tools/{i}");
    let field = format!("tools.{}", tool.id);
    if !tool.id.is_empty() && !name_ok(&tool.id) {
        invalid(
            found,
            field.clone(),
            "tool names are lowercase letters, digits and underscores",
            format!("{at}/id"),
        );
    }
    if tool
        .command
        .first()
        .is_none_or(|program| program.trim().is_empty())
    {
        invalid(
            found,
            format!("{field}.command"),
            "no program",
            format!("{at}/command"),
        );
    }
    if tool.args.get("type").and_then(|kind| kind.as_str()) != Some("object") {
        invalid(
            found,
            format!("{field}.args"),
            "the argument schema must be an object schema",
            format!("{at}/args"),
        );
    }
    let properties = tool.args.get("properties").and_then(|p| p.as_object());
    for (k, part) in tool.argv.iter().enumerate() {
        if let Some((name, _)) = crate::placeholder(part)
            && !properties.is_some_and(|p| p.contains_key(name))
        {
            invalid(
                found,
                format!("{field}.argv"),
                "placeholder names no argument",
                format!("{at}/argv/{k}"),
            );
        }
    }
    secrets(
        found,
        &tool.env,
        &format!("{field}.env"),
        &format!("{at}/env"),
    );
}
