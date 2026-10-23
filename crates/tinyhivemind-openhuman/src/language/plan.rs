//! Lower checked hive packages into the deployment plan `deploy` builds.
//!
//! Each package is one hive. A seat id several packages declare is one agent,
//! whose integration must agree across them; its `permissions` and `context`
//! may differ and become that hive's membership overlay.
use super::PackageSource;
use crate::config::{
    self, ConfigError, HiveConfig, HiveSpec, Membership, PermissionProfile, RuntimeSection,
    WorkflowTarget,
};
use std::{collections::BTreeMap, path::PathBuf};
use tinyhivemind_lang as lang;

/// What a seat runs with beyond the native agent definition.
#[derive(Clone, Debug)]
pub(crate) struct SeatPlan {
    /// The root of the package that declared the seat first.
    pub(crate) root: PathBuf,
    /// The id of that package's hive.
    pub(crate) hive: String,
    /// Executable tools the seat may call.
    pub(crate) programs: Vec<lang::ExecTool>,
    /// Hooks on the seat's turns.
    pub(crate) hooks: Vec<lang::LoweredHook>,
    /// Feature flags of the package.
    pub(crate) features: BTreeMap<String, bool>,
    /// Skill bundle ids the seat selected, and every bundle the package ships.
    pub(crate) skills: (Vec<String>, Vec<String>),
}

/// A deployment plan and the settings the plan types do not carry.
#[derive(Clone, Debug)]
pub(crate) struct Plan {
    /// The native deployment plan.
    pub(crate) config: HiveConfig,
    /// Per-seat extras, keyed by seat id.
    pub(crate) seats: BTreeMap<String, SeatPlan>,
    /// Agent session settings of the first package.
    pub(crate) session: lang::SessionSettings,
    /// Jev settings of the first package.
    pub(crate) jev: lang::JevSettings,
}

/// The id of the native profile a seat's resolved integration becomes.
pub(crate) fn seat_profile(seat: &str) -> String {
    format!("seat-{seat}")
}

fn secret(reference: &lang::SecretRef) -> config::SecretRef {
    match reference {
        lang::SecretRef::Env(name) => config::SecretRef::Env(name.clone()),
        lang::SecretRef::Store(key) => config::SecretRef::Store(key.clone()),
    }
}

fn secrets(map: &BTreeMap<String, lang::SecretRef>) -> BTreeMap<String, config::SecretRef> {
    map.iter().map(|(k, v)| (k.clone(), secret(v))).collect()
}

fn provider(provider: Option<&lang::Provider>) -> config::Provider {
    provider.map_or_else(config::Provider::default, |p| config::Provider {
        kind: match p.kind {
            lang::ProviderKind::Inherit => String::new(),
            lang::ProviderKind::Openai => "openai".into(),
            lang::ProviderKind::Openrouter => "openrouter".into(),
            lang::ProviderKind::Anthropic => "anthropic".into(),
        },
        endpoint: p.endpoint.clone(),
        credential: p.credential.as_ref().map(secret),
    })
}

fn model(model: Option<&lang::LoweredModel>) -> config::Model {
    model.map_or_else(config::Model::default, |m| config::Model {
        name: Some(m.model.name.clone()),
        temperature: m.model.temperature,
        max_tokens: m.model.max_tokens,
    })
}

fn limits(limits: &lang::Limits) -> config::Limits {
    config::Limits {
        timeout_ms: limits.timeout_ms,
        iterations: limits.iterations,
        budget_usd: limits.budget_usd,
    }
}

fn access(access: lang::Access) -> config::Access {
    match access {
        lang::Access::ReadOnly => config::Access::ReadOnly,
        lang::Access::Supervised => config::Access::Supervised,
        lang::Access::Autonomous => config::Access::Autonomous,
    }
}

fn sandbox(sandbox: lang::Sandbox) -> config::Sandbox {
    match sandbox {
        lang::Sandbox::None => config::Sandbox::None,
        lang::Sandbox::ReadOnly => config::Sandbox::ReadOnly,
        lang::Sandbox::Sandboxed => config::Sandbox::Sandboxed,
    }
}

fn mcp(server: &lang::McpServer) -> config::McpServer {
    config::McpServer {
        id: server.id.clone(),
        transport: match server.transport {
            lang::McpTransport::Stdio => "stdio".into(),
            lang::McpTransport::Http => "http".into(),
        },
        endpoint: server.endpoint.clone(),
        args: server.args.clone(),
        env: secrets(&server.env),
        headers: secrets(&server.headers),
        tools: server.tools.clone(),
        deny_tools: server.deny_tools.clone(),
    }
}

fn permission(set: &lang::PermissionSet) -> Result<PermissionProfile, ConfigError> {
    let tool_rules = if set.tool_rules.is_null() {
        None
    } else {
        Some(serde_json::from_value(set.tool_rules.clone()).map_err(|_| {
            ConfigError::UnsupportedSetting(format!("permissions.{}.tool_rules", set.id))
        })?)
    };
    Ok(PermissionProfile {
        id: set.id.clone(),
        access: access(set.access),
        sandbox: sandbox(set.sandbox),
        allow_tools: set.allow_tools.clone(),
        deny_tools: set.deny_tools.clone(),
        send_destinations: set.send_destinations.clone(),
        send_operations: set.send_operations.clone(),
        approval: None,
        tool_rules,
    })
}

fn runtime(source: &PackageSource) -> RuntimeSection {
    let hive = source.lowered();
    let integrations = &hive.integrations;
    let declared = &integrations.runtime;
    RuntimeSection {
        deployment_id: declared
            .deployment_id
            .clone()
            .unwrap_or_else(|| hive.id.clone()),
        memory_root: None,
        provider: provider(integrations.provider.as_ref()),
        model: model(integrations.model.as_ref()),
        workspace: declared.workspace.clone(),
        session_store: match declared.session_store {
            lang::SessionStore::Memory => "memory".into(),
            lang::SessionStore::Sqlite => "sqlite".into(),
            lang::SessionStore::Host => "host".into(),
        },
        memory_engine: match declared.memory_engine {
            lang::MemoryEngine::Disabled => "disabled".into(),
            lang::MemoryEngine::Memory => "memory".into(),
            lang::MemoryEngine::Host => "host".into(),
        },
        permission_profile: declared.permissions.clone(),
        sandbox: sandbox(declared.sandbox),
        tool_rules: None,
        mcp: Vec::new(),
        skills: Vec::new(),
        autonomy: access(declared.autonomy),
        limits: limits(&declared.limits),
        concurrency: declared.concurrency,
        include_user_skills: declared.include_user_skills,
        max_agents: declared.max_agents,
        start_scheduler: declared.start_scheduler,
    }
}

/// A native profile for one seat's resolved integration.
fn profile(
    hive: &lang::LoweredHive,
    seat: &lang::LoweredSeat,
    skills: Option<String>,
) -> config::Profile {
    let integration = &seat.integration;
    // Context is per hive: it reaches a turn through the seat's membership,
    // so a seat several hives share sees each hive's own documents.
    let _ = hive;
    let prompt = seat.prompt.clone();
    let mut named = integration.tools.allow.clone();
    if !named.is_empty() {
        named.extend(integration.programs.iter().map(|tool| tool.id.clone()));
    }
    config::Profile {
        id: seat_profile(&seat.id),
        definition_base: integration.definition_base.clone(),
        bare_prompt: integration.bare_prompt,
        include_user_skills: None,
        system_prompt: prompt,
        context: Vec::new(),
        model: model(integration.model.as_ref()),
        permissions: None,
        tool_scope: if named.is_empty() {
            config::ToolScope::Wildcard
        } else {
            config::ToolScope::Named(named)
        },
        deny_tools: integration.tools.deny.clone(),
        mcp: integration.mcp.iter().map(mcp).collect(),
        skills: skills.into_iter().collect(),
        subagents: integration.subagents.clone(),
        memory: None,
        limits: limits(&integration.limits),
        groups: integration.groups.clone(),
    }
}

/// A native profile for a declared language profile, used as a subagent.
fn subagent(package: &lang::Package, declared: &lang::Profile) -> config::Profile {
    let mut prompt = String::new();
    for path in &declared.context {
        if let Some(body) = package.documents.get(path) {
            prompt.push_str(body);
            prompt.push('\n');
        }
    }
    let m = &package.manifest;
    let model = declared
        .model
        .as_ref()
        .and_then(|id| m.models.iter().find(|candidate| &candidate.id == id))
        .map_or_else(config::Model::default, |found| config::Model {
            name: Some(found.name.clone()),
            temperature: found.temperature,
            max_tokens: found.max_tokens,
        });
    config::Profile {
        id: declared.id.clone(),
        definition_base: declared.definition_base.clone(),
        bare_prompt: declared.bare_prompt,
        system_prompt: prompt.trim_end().to_owned(),
        model,
        permissions: declared.permissions.clone(),
        tool_scope: if declared.tools.allow.is_empty() {
            config::ToolScope::Wildcard
        } else {
            config::ToolScope::Named(declared.tools.allow.clone())
        },
        deny_tools: declared.tools.deny.clone(),
        subagents: declared.subagents.clone(),
        limits: limits(&declared.limits),
        ..config::Profile::default()
    }
}

/// Insert `value` under `id`, or require the one already there to equal it.
fn agree<T: serde::Serialize>(
    map: &mut BTreeMap<String, T>,
    section: &str,
    id: &str,
    value: T,
) -> Result<(), ConfigError> {
    let same = |a: &T, b: &T| serde_json::to_value(a).ok() == serde_json::to_value(b).ok();
    match map.get(id) {
        Some(existing) if !same(existing, &value) => Err(ConfigError::ConflictingDeclaration {
            section: section.into(),
            id: id.into(),
        }),
        Some(_) => Ok(()),
        None => {
            map.insert(id.into(), value);
            Ok(())
        }
    }
}

/// The seats one package declares: each new seat's native profile and plan
/// entry, a seat already declared checked for agreement, and the package's
/// membership list.
fn seats_of(
    source: &PackageSource,
    config: &mut HiveConfig,
    profiles: &mut BTreeMap<String, config::Profile>,
    seats: &mut Seats,
) -> Result<Vec<Membership>, ConfigError> {
    let package = source.package();
    let hive = source.lowered();
    let shipped: Vec<String> = hive
        .integrations
        .skills
        .iter()
        .map(|s| s.id.clone())
        .collect();
    let mut members = Vec::new();
    for (seat, declared) in hive.seats.iter().zip(&package.manifest.seats) {
        let mut shared = seat.integration.clone();
        shared.permissions.clear();
        shared.context.clear();
        let selected: Vec<String> = seat
            .integration
            .skills
            .iter()
            .map(|s| s.id.clone())
            .collect();
        let extras = SeatPlan {
            root: source.root().to_path_buf(),
            hive: hive.id.clone(),
            programs: seat.integration.programs.clone(),
            hooks: seat.integration.hooks.clone(),
            features: hive.integrations.features.clone(),
            skills: (selected, shipped.clone()),
        };
        match seats.get(&seat.id) {
            Some((existing, prompt, _)) if existing != &shared || prompt != &seat.prompt => {
                return Err(ConfigError::ConflictingSeat(seat.id.clone()));
            }
            Some(_) => {}
            None => {
                let mut native = profile(hive, seat, None);
                native.include_user_skills = Some(hive.integrations.runtime.include_user_skills);
                agree(profiles, "profiles", &native.id.clone(), native)?;
                seats.insert(seat.id.clone(), (shared, seat.prompt.clone(), extras));
                config.seats.push(config::Seat {
                    id: seat.id.clone(),
                    agent_id: seat.integration.agent_id.clone(),
                    profile: seat_profile(&seat.id),
                    overrides: config::SeatOverrides::default(),
                });
            }
        }
        members.push(Membership {
            seat: seat.id.clone(),
            role: declared.role.clone(),
            context: Vec::new(),
            permission_profile: declared.permissions.clone(),
            mcp: None,
        });
    }
    Ok(members)
}

/// Seats declared so far: shared integration, prompt and plan, by seat id.
type Seats = BTreeMap<String, (lang::LoweredSeatIntegration, String, SeatPlan)>;

/// Lower `packages` into one deployment plan.
///
/// # Errors
/// [`ConfigError::ConflictingSeat`] or
/// [`ConfigError::ConflictingDeclaration`] when packages disagree, and
/// [`ConfigError::UnsupportedSetting`] for tool rules the harness cannot
/// decode. An empty list is [`ConfigError::EmptyHive`].
pub(crate) fn plan(packages: &[PackageSource]) -> Result<Plan, ConfigError> {
    let first = packages
        .first()
        .ok_or_else(|| ConfigError::EmptyHive("deployment".into()))?;
    let mut config = HiveConfig {
        runtime: runtime(first),
        ..HiveConfig::default()
    };
    let mut profiles = BTreeMap::new();
    let mut permissions = BTreeMap::new();
    let mut seats = Seats::new();
    for source in packages {
        if runtime(source).deployment_id != config.runtime.deployment_id
            || source.package().manifest.runtime != first.package().manifest.runtime
        {
            return Err(ConfigError::ConflictingDeclaration {
                section: "runtime".into(),
                id: "runtime".into(),
            });
        }
        let package = source.package();
        let hive = source.lowered();
        for set in &package.manifest.permissions {
            agree(&mut permissions, "permissions", &set.id, permission(set)?)?;
        }
        for declared in &package.manifest.profiles {
            agree(
                &mut profiles,
                "profiles",
                &declared.id,
                subagent(package, declared),
            )?;
        }
        let members = seats_of(source, &mut config, &mut profiles, &mut seats)?;
        let policies = &hive.policies;
        config.hives.push(HiveSpec {
            id: hive.id.clone(),
            name: hive.name.clone(),
            members,
            memory_root: None,
            episode: policies.episode,
            routing: policies.routing.clone(),
            coordinator: super::coordinator_options(&hive.coordinator),
            division: policies.division,
            conduct: policies.conduct,
        });
        for job in &hive.integrations.workflows {
            config.workflows.push(config::Workflow {
                id: job.id.clone(),
                schedule: job.schedule.clone(),
                target: match &job.target {
                    lang::WorkflowTarget::Seat(seat) => WorkflowTarget::Seat(seat.clone()),
                    lang::WorkflowTarget::Hive(hive) => WorkflowTarget::Hive(hive.clone()),
                },
                prompt: job.prompt.clone(),
                enabled: job.enabled,
                retries: job.retries,
                single_flight: job.single_flight,
            });
        }
    }
    config.profiles = profiles.into_values().collect();
    config.permission_profiles = permissions.into_values().collect();
    let lowered = first.lowered();
    Ok(Plan {
        config,
        seats: seats
            .into_iter()
            .map(|(id, (_, _, plan))| (id, plan))
            .collect(),
        session: lowered.integrations.session.clone(),
        jev: lowered.integrations.jev.clone(),
    })
}

#[cfg(test)]
#[path = "plan_test.rs"]
mod test;
