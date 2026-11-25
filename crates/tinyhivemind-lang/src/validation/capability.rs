//! The capability table, judged seat by seat: every setting a seat resolves
//! to that its harness kind rejects is an [`Error::Integration`] at the
//! field that sets it.
//!
//! The table is [`HarnessKind::support`]; this module only finds where each
//! row's setting lives. A row that moves to supported stops being reported
//! here with no change to this file. A seat whose harness reference does not
//! resolve is skipped: `harness.rs` has already reported it.
use crate::{
    Capability, Error, GroupMode, HarnessKind, Manifest, Package, ProviderKind, Sandbox, Seat,
    Support,
    diagnostic::{Findings, HIVE, escape},
    harness::resolve,
};
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
            out.push(
                Error::Integration {
                    field: format!("harnesses.{}.isolation", harness.id),
                    reason: "seats sharing one openclaw instance must resolve to one provider; \
                             use isolation seat"
                        .into(),
                },
                format!("/harnesses/{h}/isolation"),
            );
        }
    }
    for (error, at) in out.findings {
        found.push(error, HIVE, at);
    }
}

/// Findings in the order met, one per location: a setting several seats
/// share is reported once, for the first seat that resolves to it.
#[derive(Default)]
struct Out {
    seen: BTreeSet<String>,
    findings: Vec<(Error, String)>,
}

impl Out {
    fn push(&mut self, error: Error, at: String) {
        if self.seen.insert(at.clone()) {
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
            self.found
                .push(Error::Integration { field, reason }, at);
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
            ProviderKind::Anthropic => {
                self.reject(Capability::Anthropic, field("kind"), at("kind"));
            }
            ProviderKind::Openai if provider.endpoint.is_none() => self.reject(
                Capability::OpenaiDefaultEndpoint,
                field("endpoint"),
                at("endpoint"),
            ),
            ProviderKind::Openai => {}
        }
        if provider.credential.is_some() {
            self.reject(
                Capability::ProviderCredential,
                field("credential"),
                at("credential"),
            );
        }
    }

    /// What the runtime, the seat's profile and the seat layer onto it:
    /// permission sets, toolpacks and the tools they grant, delegation, and
    /// the iteration limit.
    fn layers(&mut self, seat: &Seat, i: usize) {
        let m = self.m;
        let p = profile(m, seat);
        let from_profile = p.map(|k| &m.profiles[k]);
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
            let set = &m.permissions[k];
            if set.sandbox != Sandbox::None {
                let field = format!("permissions.{}.sandbox", set.id);
                self.reject(
                    Capability::Sandbox,
                    field,
                    format!("/permissions/{k}/sandbox"),
                );
            }
            if !set.tool_rules.is_null() {
                let field = format!("permissions.{}.tool_rules", set.id);
                let at = format!("/permissions/{k}/tool_rules");
                self.reject(Capability::BuiltinToolRules, field, at);
            }
        }
        let packs = m
            .runtime
            .toolpacks
            .iter()
            .chain(from_profile.into_iter().flat_map(|p| &p.toolpacks))
            .chain(&seat.toolpacks);
        for k in indices(&m.toolpacks, packs, |v| &v.id) {
            let pack = &m.toolpacks[k];
            for (group, mode) in &pack.groups {
                if *mode != GroupMode::Off {
                    let field = format!("toolpacks.{}.groups.{group}", pack.id);
                    let at = format!("/toolpacks/{k}/groups/{}", escape(group));
                    self.reject(Capability::BuiltinToolGroups, field, at);
                }
            }
            for t in indices(&m.tools, &pack.tools, |v| &v.id) {
                self.reserved(t);
            }
        }
        if let (Some(k), Some(declared)) = (p, from_profile) {
            if !declared.subagents.is_empty() {
                let field = format!("profiles.{}.subagents", declared.id);
                self.reject(
                    Capability::Subagents,
                    field,
                    format!("/profiles/{k}/subagents"),
                );
            }
            if declared.definition_base.is_some() {
                let field = format!("profiles.{}.definition_base", declared.id);
                let at = format!("/profiles/{k}/definition_base");
                self.reject(Capability::DefinitionBase, field, at);
            }
        }
        self.iterations(seat, i, p);
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

    /// The seat's resolved `limits.iterations` against the kind's fixed cap,
    /// reported at whichever layer set it.
    fn iterations(&mut self, seat: &Seat, i: usize, p: Option<usize>) {
        let m = self.m;
        let Some(cap) = self.kind.iteration_cap() else {
            return;
        };
        let model = model_id(m, seat).and_then(|id| m.models.iter().position(|v| &v.id == id));
        let layers = [
            Some((&seat.limits, format!("/seats/{i}"))),
            p.map(|k| (&m.profiles[k].limits, format!("/profiles/{k}"))),
            model.map(|k| (&m.models[k].limits, format!("/models/{k}"))),
            Some((&m.runtime.limits, "/runtime".to_owned())),
        ];
        let set = layers
            .into_iter()
            .flatten()
            .find_map(|(limits, at)| Some((limits.iterations?, at)));
        if let Some((iterations, at)) = set
            && iterations > cap
        {
            self.found.push(
                Error::Integration {
                    field: format!("seats.{}.limits.iterations", seat.id),
                    reason: format!(
                        "the {} harness ends a turn after {cap} model iterations",
                        self.kind
                    ),
                },
                format!("{at}/limits/iterations"),
            );
        }
    }

    /// Hive-wide settings every seat runs under.
    fn hive(&mut self) {
        let m = self.m;
        let r = &m.runtime;
        if r.sandbox != Sandbox::None {
            let at = "/runtime/sandbox".to_owned();
            self.reject(Capability::Sandbox, "runtime.sandbox".into(), at);
        }
        if r.session_store == crate::SessionStore::Host {
            let at = "/runtime/session_store".to_owned();
            self.reject(
                Capability::HostSessionStore,
                "runtime.session_store".into(),
                at,
            );
        }
        if r.memory_engine == crate::MemoryEngine::Host {
            let at = "/runtime/memory_engine".to_owned();
            self.reject(
                Capability::HostMemoryEngine,
                "runtime.memory_engine".into(),
                at,
            );
        }
        if m.jev != crate::JevSettings::default() {
            self.reject(Capability::Jev, "jev".into(), "/jev".into());
        }
        let s = &m.session;
        for (set, key) in [
            (s.compact_context.is_some(), "compact_context"),
            (s.max_history_messages.is_some(), "max_history_messages"),
            (
                s.tool_result_budget_bytes.is_some(),
                "tool_result_budget_bytes",
            ),
            (s.max_tool_iterations.is_some(), "max_tool_iterations"),
            (s.parallel_tools.is_some(), "parallel_tools"),
            (s.agents_md.is_some(), "agents_md"),
        ] {
            if set {
                let field = format!("session.{key}");
                self.reject(Capability::SessionTuning, field, format!("/session/{key}"));
            }
        }
    }
}
