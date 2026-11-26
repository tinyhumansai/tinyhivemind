//! The capability table, judged seat by seat: every setting a seat resolves
//! to that its harness kind rejects is an [`Error::Integration`] at the
//! field that sets it.
//!
//! The judgement is default-deny. Every object a seat runs with (the
//! runtime, the seat, its profile, model, permission sets, toolpacks and
//! hooks, and the session and Jev settings) is walked field by field against
//! its default, and a field set to anything else asks for the capability
//! `harness/fields.rs` assigns it; the table is [`HarnessKind::support`]. A
//! row that moves to supported stops being reported with no change here. The
//! fields judged by value (provider shape, sandbox, stores, tool rules and
//! groups, a hook's event, the iteration limit) are handled by hand. A seat
//! whose harness reference does not resolve is skipped: `harness.rs` has
//! already reported it.
use crate::{
    Capability, Error, GroupMode, HarnessKind, HookEvent, Manifest, MemoryEngine, Package,
    ProviderKind, Sandbox, Seat, SessionStore, Support,
    diagnostic::{Findings, HIVE, escape},
    harness::{
        fields::{Field, field},
        resolve,
    },
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Arguments the host adds to every served tool for call identity.
const RESERVED_ARGUMENTS: [&str; 2] = ["turn", "call"];

/// Judge every seat of `package`, then every shared instance.
pub(super) fn capabilities(package: &Package, found: &mut Findings) {
    let m = &package.manifest;
    let mut providers: BTreeMap<usize, BTreeSet<&str>> = BTreeMap::new();
    let mut out = Out::default();
    for (i, seat) in m.seats.iter().enumerate() {
        let kind = match resolve::named(m, seat) {
            None => HarnessKind::OpenHuman,
            Some((id, _)) => match m.harnesses.iter().position(|h| h.id == id) {
                Some(h) => {
                    if let Some(provider) = provider_id(m, seat) {
                        providers.entry(h).or_default().insert(provider);
                    }
                    m.harnesses[h].kind
                }
                None => continue,
            },
        };
        let mut judge = Judge {
            m,
            kind,
            found: &mut out,
        };
        judge.provider(seat);
        judge.layers(seat, i);
        judge.hive();
    }
    for (h, seen) in providers {
        let harness = &m.harnesses[h];
        if harness.kind == HarnessKind::OpenClaw
            && harness.resolved_isolation() == crate::Isolation::Deployment
            && seen.len() > 1
        {
            // At the isolation that shares the instance, or at the entry
            // when it is the kind's default and has no field of its own.
            let at = if harness.isolation.is_some() {
                format!("/harnesses/{h}/isolation")
            } else {
                format!("/harnesses/{h}")
            };
            out.push(
                Error::Integration {
                    field: format!("harnesses.{}.isolation", harness.id),
                    reason: "seats sharing one openclaw instance must resolve to one provider; \
                             use isolation seat"
                        .into(),
                },
                at,
            );
        }
    }
    for (error, at) in out.findings {
        found.push(error, HIVE, at);
    }
}

/// Findings in the order met, one per location and message: a setting
/// several seats share is reported once per harness kind that rejects it,
/// each with its own kind's reason.
#[derive(Default)]
struct Out {
    seen: BTreeSet<(String, String)>,
    findings: Vec<(Error, String)>,
}

impl Out {
    fn push(&mut self, error: Error, at: String) {
        if self.seen.insert((at.clone(), error.to_string())) {
            self.findings.push((error, at));
        }
    }
}

/// The model id `seat` resolves to: its own, its profile's, the runtime's.
fn model_id<'a>(m: &'a Manifest, seat: &'a Seat) -> Option<&'a String> {
    seat.model
        .as_ref()
        .or_else(|| profile(m, seat).and_then(|k| m.profiles[k].model.as_ref()))
        .or(m.runtime.model.as_ref())
}

/// The provider id `seat` resolves to: its model's, else the runtime's.
fn provider_id<'a>(m: &'a Manifest, seat: &'a Seat) -> Option<&'a str> {
    let model = model_id(m, seat).and_then(|id| m.models.iter().find(|v| &v.id == id));
    model
        .and_then(|model| model.provider.as_ref())
        .or(m.runtime.provider.as_ref())
        .map(String::as_str)
}

/// The index of `seat`'s profile in `profiles`.
fn profile(m: &Manifest, seat: &Seat) -> Option<usize> {
    let id = seat.profile.as_ref()?;
    m.profiles.iter().position(|p| &p.id == id)
}

/// The indices of the declarations `ids` name, skipping unresolved ones.
fn indices<'a, T>(
    items: &[T],
    ids: impl IntoIterator<Item = &'a String>,
    key: impl Fn(&T) -> &str,
) -> BTreeSet<usize> {
    ids.into_iter()
        .filter_map(|id| items.iter().position(|item| key(item) == id))
        .collect()
}

/// One seat's judgement against its kind's column of the table.
struct Judge<'a, 'f> {
    m: &'a Manifest,
    kind: HarnessKind,
    found: &'f mut Out,
}

impl Judge<'_, '_> {
    /// Report `capability` at `at` when the kind rejects it.
    fn reject(&mut self, capability: Capability, field: String, at: String) {
        if let Support::Rejected(unlock) = self.kind.support(capability) {
            let reason = format!(
                "the {} harness cannot honour {capability}: {unlock}",
                self.kind
            );
            self.found.push(Error::Integration { field, reason }, at);
        }
    }

    /// The provider rows: the seat's resolved provider, or the field that
    /// should have named one.
    fn provider(&mut self, seat: &Seat) {
        let m = self.m;
        let (named, at, field) = match model_id(m, seat) {
            None => (
                m.runtime.provider.as_ref(),
                "/runtime/provider".to_owned(),
                "runtime.provider".to_owned(),
            ),
            Some(id) => {
                let Some(k) = m.models.iter().position(|model| &model.id == id) else {
                    return;
                };
                (
                    m.models[k]
                        .provider
                        .as_ref()
                        .or(m.runtime.provider.as_ref()),
                    format!("/models/{k}/provider"),
                    format!("models.{id}.provider"),
                )
            }
        };
        let Some(named) = named else {
            self.reject(Capability::InheritedProvider, field, at);
            return;
        };
        let Some(p) = m.providers.iter().position(|v| &v.id == named) else {
            return;
        };
        let provider = &m.providers[p];
        let at = |key: &str| format!("/providers/{p}/{key}");
        let field = |key: &str| format!("providers.{named}.{key}");
        match provider.kind {
            ProviderKind::Inherit => {
                self.reject(Capability::InheritedProvider, field("kind"), at("kind"));
            }
            ProviderKind::Openrouter => {
                self.reject(Capability::Openrouter, field("kind"), at("kind"));
            }
            ProviderKind::Anthropic if provider.endpoint.is_none() => {
                self.reject(Capability::Anthropic, field("kind"), at("kind"));
            }
            ProviderKind::Anthropic => {
                self.reject(Capability::AnthropicEndpoint, field("kind"), at("kind"));
            }
            ProviderKind::Openai if provider.endpoint.is_none() => self.reject(
                Capability::OpenaiDefaultEndpoint,
                field("endpoint"),
                at("endpoint"),
            ),
            ProviderKind::Openai => {
                self.reject(Capability::OpenaiEndpoint, field("kind"), at("kind"));
            }
        }
        if provider.credential.is_some() {
            self.reject(
                Capability::ProviderCredential,
                field("credential"),
                at("credential"),
            );
        }
    }

    /// Walk `value` against `default` as `owner`'s fields: a judged field
    /// that differs asks for its capability, a nested one is walked in turn.
    fn walk(&mut self, owner: &str, value: &Value, default: &Value, at: &str, name: &str) {
        let Some(fields) = value.as_object() else {
            return;
        };
        for (key, value) in fields {
            let base = default.get(key).unwrap_or(&Value::Null);
            let (at, name) = (format!("{at}/{}", escape(key)), format!("{name}.{key}"));
            match field(owner, key) {
                Some(Field::Nested(inner)) => self.walk(inner, value, base, &at, &name),
                Some(Field::Judged(capability)) if value != base => {
                    self.reject(capability, name, at);
                }
                // Default-deny: a field with no decision is never passed.
                None => self.reject(Capability::Undecided, name, at),
                Some(Field::Judged(_) | Field::Structure | Field::Special) => {}
            }
        }
    }

    /// Walk one declaration of `owner` against its default.
    fn declared<T: serde::Serialize>(
        &mut self,
        owner: &str,
        value: &T,
        base: &T,
        at: &str,
        name: &str,
    ) {
        let (value, base) = (wire(value), wire(base));
        self.walk(owner, &value, &base, at, name);
    }

    /// What the runtime, the seat's profile and the seat layer onto it:
    /// every field of each, permission sets, toolpacks and the tools they
    /// grant, hooks, and the iteration limit.
    fn layers(&mut self, seat: &Seat, i: usize) {
        let m = self.m;
        let p = profile(m, seat);
        let from_profile = p.map(|k| &m.profiles[k]);
        let seat_default = Seat {
            config: Value::Object(serde_json::Map::new()),
            ..Seat::default()
        };
        self.declared(
            "seat",
            seat,
            &seat_default,
            &format!("/seats/{i}"),
            &format!("seats.{}", seat.id),
        );
        if let (Some(k), Some(declared)) = (p, from_profile) {
            let name = format!("profiles.{}", declared.id);
            self.declared(
                "profile",
                declared,
                &crate::Profile::default(),
                &format!("/profiles/{k}"),
                &name,
            );
        }
        if let Some(k) = model_id(m, seat).and_then(|id| m.models.iter().position(|v| &v.id == id))
        {
            let model = &m.models[k];
            let name = format!("models.{}", model.id);
            self.declared(
                "model",
                model,
                &crate::Model::default(),
                &format!("/models/{k}"),
                &name,
            );
        }
        let permissions = indices(
            &m.permissions,
            [
                m.runtime.permissions.as_ref(),
                from_profile.and_then(|p| p.permissions.as_ref()),
                seat.permissions.as_ref(),
            ]
            .into_iter()
            .flatten(),
            |v| &v.id,
        );
        for k in permissions {
            self.permission_set(k);
        }
        let packs = indices(
            &m.toolpacks,
            m.runtime
                .toolpacks
                .iter()
                .chain(from_profile.into_iter().flat_map(|p| &p.toolpacks))
                .chain(&seat.toolpacks),
            |v| &v.id,
        );
        for k in &packs {
            self.toolpack(*k);
        }
        let packs: Vec<&crate::Toolpack> = packs.into_iter().map(|k| &m.toolpacks[k]).collect();
        let mcp: Vec<&String> = from_profile
            .into_iter()
            .flat_map(|p| &p.mcp)
            .chain(&seat.mcp)
            .chain(packs.iter().flat_map(|pack| &pack.mcp))
            .collect();
        let skills: Vec<&String> = from_profile
            .into_iter()
            .flat_map(|p| &p.skills)
            .chain(&seat.skills)
            .chain(packs.iter().flat_map(|pack| &pack.skills))
            .collect();
        self.reached(&mcp, &skills);
        self.hooks(
            from_profile
                .into_iter()
                .flat_map(|p| &p.hooks)
                .chain(&seat.hooks),
        );
        self.iterations(seat, i, p);
    }

    /// The hooks that run on the seat's turns, each walked at its
    /// declaration; a `stop` hook is rejected.
    fn hooks<'h>(&mut self, ids: impl IntoIterator<Item = &'h String>) {
        let m = self.m;
        for k in indices(&m.hooks, ids, |v| &v.id) {
            let hook = &m.hooks[k];
            let name = format!("hooks.{}", hook.id);
            let at = format!("/hooks/{k}");
            self.declared("hook", hook, &crate::Hook::default(), &at, &name);
            if hook.event == HookEvent::Stop {
                let (field, at) = (format!("{name}.event"), format!("{at}/event"));
                self.reject(Capability::StopHooks, field, at);
            }
        }
    }

    /// One permission set that applies to the seat.
    fn permission_set(&mut self, k: usize) {
        let layer = &self.m.permissions[k];
        let name = format!("permissions.{}", layer.id);
        let at = format!("/permissions/{k}");
        self.declared(
            "permissions",
            layer,
            &crate::PermissionSet::default(),
            &at,
            &name,
        );
        if layer.sandbox != Sandbox::None {
            self.reject(
                Capability::Sandbox,
                format!("{name}.sandbox"),
                format!("{at}/sandbox"),
            );
        }
        if !layer.tool_rules.is_null() {
            let (field, at) = (format!("{name}.tool_rules"), format!("{at}/tool_rules"));
            self.reject(Capability::BuiltinToolRules, field, at);
        }
    }

    /// One toolpack the seat receives, and the tools it grants.
    fn toolpack(&mut self, k: usize) {
        let m = self.m;
        let pack = &m.toolpacks[k];
        let name = format!("toolpacks.{}", pack.id);
        self.declared(
            "toolpack",
            pack,
            &crate::Toolpack::default(),
            &format!("/toolpacks/{k}"),
            &name,
        );
        for (group, mode) in &pack.groups {
            if *mode != GroupMode::Off {
                let field = format!("{name}.groups.{group}");
                let at = format!("/toolpacks/{k}/groups/{}", escape(group));
                self.reject(Capability::BuiltinToolGroups, field, at);
            }
        }
        for t in indices(&m.tools, &pack.tools, |v| &v.id) {
            let tool = &m.tools[t];
            let name = format!("tools.{}", tool.id);
            self.declared("tool", tool, tool, &format!("/tools/{t}"), &name);
            self.reserved(t);
        }
    }

    /// The MCP servers and skills a seat reaches, by id, each walked at its
    /// declaration.
    fn reached(&mut self, mcp: &[&String], skills: &[&String]) {
        let m = self.m;
        for k in indices(&m.mcp, mcp.iter().copied(), |v| &v.id) {
            let server = &m.mcp[k];
            let name = format!("mcp.{}", server.id);
            self.declared(
                "mcp",
                server,
                &crate::McpServer::default(),
                &format!("/mcp/{k}"),
                &name,
            );
        }
        for k in indices(&m.skills, skills.iter().copied(), |v| &v.id) {
            let skill = &m.skills[k];
            let name = format!("skills.{}", skill.id);
            self.declared(
                "skill",
                skill,
                &crate::Skill::default(),
                &format!("/skills/{k}"),
                &name,
            );
        }
    }

    /// An executable tool whose own arguments use a name the host reserves.
    fn reserved(&mut self, t: usize) {
        let tool = &self.m.tools[t];
        let Some(properties) = tool.args.get("properties").and_then(|p| p.as_object()) else {
            return;
        };
        for name in RESERVED_ARGUMENTS {
            if properties.contains_key(name) {
                self.found.push(
                    Error::Integration {
                        field: format!("tools.{}.args.{name}", tool.id),
                        reason: "turn and call are added by the host to every served tool".into(),
                    },
                    format!("/tools/{t}/args/properties/{name}"),
                );
            }
        }
    }

    /// The seat's resolved `limits.iterations`, reported at whichever layer
    /// set it: rejected where the kind has no iteration limit, and above the
    /// kind's fixed cap where it has one.
    fn iterations(&mut self, seat: &Seat, i: usize, p: Option<usize>) {
        let m = self.m;
        let model = model_id(m, seat).and_then(|id| m.models.iter().position(|v| &v.id == id));
        let layers = [
            Some((&seat.limits, format!("/seats/{i}"))),
            p.map(|k| (&m.profiles[k].limits, format!("/profiles/{k}"))),
            model.map(|k| (&m.models[k].limits, format!("/models/{k}"))),
            Some((&m.runtime.limits, "/runtime".to_owned())),
        ];
        let Some((iterations, at)) = layers
            .into_iter()
            .flatten()
            .find_map(|(limits, at)| Some((limits.iterations?, format!("{at}/limits/iterations"))))
        else {
            return;
        };
        let field = format!("seats.{}.limits.iterations", seat.id);
        self.reject(Capability::IterationLimit, field.clone(), at.clone());
        if let Some(cap) = self.kind.iteration_cap()
            && iterations > cap
        {
            let reason = format!(
                "the {} harness ends a turn after {cap} model iterations",
                self.kind
            );
            self.found.push(Error::Integration { field, reason }, at);
        }
    }

    /// Hive-wide settings every seat runs under.
    fn hive(&mut self) {
        let m = self.m;
        let r = &m.runtime;
        self.declared(
            "runtime",
            r,
            &crate::RuntimeSettings::default(),
            "/runtime",
            "runtime",
        );
        if r.sandbox != Sandbox::None {
            self.reject(
                Capability::Sandbox,
                "runtime.sandbox".into(),
                "/runtime/sandbox".into(),
            );
        }
        let store = match r.session_store {
            SessionStore::Memory => None,
            SessionStore::Sqlite => Some(Capability::SqliteSessions),
            SessionStore::Host => Some(Capability::HostSessionStore),
        };
        if let Some(capability) = store {
            let at = "/runtime/session_store".to_owned();
            self.reject(capability, "runtime.session_store".into(), at);
        }
        let engine = match r.memory_engine {
            MemoryEngine::Disabled => None,
            MemoryEngine::Memory => Some(Capability::InProcessMemory),
            MemoryEngine::Host => Some(Capability::HostMemoryEngine),
        };
        if let Some(capability) = engine {
            let at = "/runtime/memory_engine".to_owned();
            self.reject(capability, "runtime.memory_engine".into(), at);
        }
        self.declared(
            "session",
            &m.session,
            &crate::SessionSettings::default(),
            "/session",
            "session",
        );
        self.declared("jev", &m.jev, &crate::JevSettings::default(), "/jev", "jev");
        // The manifest's own fields (context, features, workflows), then
        // each workflow; the sections are walked where a seat reaches them.
        self.declared("manifest", m, &Manifest::default(), "", "hive");
        for (k, job) in m.workflows.iter().enumerate() {
            let name = format!("workflows.{}", job.id);
            self.declared("workflow", job, job, &format!("/workflows/{k}"), &name);
        }
    }
}

/// A package value as wire JSON; the wire types serialize infallibly, and a
/// failure would only skip the walk.
fn wire<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

#[cfg(test)]
mod test;
