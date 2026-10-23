//! Canonical serializable manifest and its runtime-independent selections.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tinyhivemind_core::{
    approval::ApprovalPolicy,
    driver::ConductPolicy,
    embed::RoutingPolicy,
    hive::{DivisionPolicy, EpisodePolicy},
};
use tinyhivemind_hives::CoordinatorOptions;

/// Credential location; never a resolved credential. The language's own.
pub(crate) use tinyhivemind_lang::SecretRef;
/// Provider endpoint and credential reference.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Provider {
    /// Provider selector; `openai` supports compatible endpoints.
    pub(crate) kind: String,
    /// Optional compatible HTTP endpoint.
    pub(crate) endpoint: Option<String>,
    /// Credential reference resolved only during deployment.
    pub(crate) credential: Option<SecretRef>,
}
/// Model and sampling defaults.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Model {
    /// Provider model identifier.
    pub(crate) name: Option<String>,
    /// Sampling temperature, including zero.
    pub(crate) temperature: Option<f64>,
    /// Maximum generated tokens per model request.
    pub(crate) max_tokens: Option<u32>,
}
/// Turn resource limits; omission inherits the parent.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Limits {
    /// Wall-clock timeout in milliseconds.
    pub(crate) timeout_ms: Option<u64>,
    /// Maximum model/tool iterations.
    pub(crate) iterations: Option<usize>,
    /// Maximum monetary model spend in dollars.
    pub(crate) budget_usd: Option<f64>,
}
/// Ordered access tiers, from least to greatest authority.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Access {
    /// No mutating tools.
    ReadOnly,
    /// Human-supervised mutation.
    #[default]
    Supervised,
    /// Autonomous mutation subject to other gates.
    Autonomous,
}
/// Shell/file confinement selection.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Sandbox {
    /// Access and path rules only.
    #[default]
    None,
    /// Disable writes and execution.
    ReadOnly,
    /// Host-provided jail or Docker backend.
    Sandboxed,
}
/// Tool visibility selector.
#[derive(Clone, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ToolScope {
    /// Every registered tool subject to permission gates.
    #[default]
    Wildcard,
    /// Only explicitly named tools.
    Named(Vec<String>),
    /// Only host-supplied tools, with read-only access.
    HostOnly,
}
/// Immutable MCP connection and server tool selection.
#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(deny_unknown_fields)]
pub(crate) struct McpServer {
    /// Unique connection name.
    pub(crate) id: String,
    /// `stdio`, `sse` or `http`.
    pub(crate) transport: String,
    /// URL for network transports, executable for stdio.
    pub(crate) endpoint: String,
    /// Stdio command arguments.
    #[serde(default)]
    pub(crate) args: Vec<String>,
    /// Credential-bearing environment values, all references.
    #[serde(default)]
    pub(crate) env: BTreeMap<String, SecretRef>,
    /// Credential-bearing HTTP headers, all references.
    #[serde(default)]
    pub(crate) headers: BTreeMap<String, SecretRef>,
    /// None exposes all server tools; Some limits them.
    #[serde(default)]
    pub(crate) tools: Option<Vec<String>>,
    /// Server tools denied independently of its allowlist.
    #[serde(default)]
    pub(crate) deny_tools: Vec<String>,
}
/// Fixed lifetime agent memory binding.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Memory {
    /// Logical namespace root; absent inherits the deployment memory root.
    /// Deployment encodes it with [`crate::deploy::native_memory_root`].
    pub(crate) root: Option<String>,
}
/// Named authority baseline or restriction.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct PermissionProfile {
    /// Unique profile id.
    pub(crate) id: String,
    /// Access tier.
    pub(crate) access: Access,
    /// Sandbox confinement.
    pub(crate) sandbox: Sandbox,
    /// None inherits unrestricted names; Some is a finite allowlist.
    pub(crate) allow_tools: Option<Vec<String>>,
    /// Tool names always denied.
    pub(crate) deny_tools: Vec<String>,
    /// Allowed direct/hive destination ids; None is unrestricted.
    pub(crate) send_destinations: Option<Vec<String>>,
    /// Allowed send operations; None is unrestricted.
    pub(crate) send_operations: Option<Vec<String>>,
    /// Native total core approval policy; omission inherits.
    pub(crate) approval: Option<ApprovalPolicy>,
    /// Native argument-sensitive tool rules; omission inherits.
    pub(crate) tool_rules: Option<tinytools::ToolRules>,
}
/// Global runtime baseline, shared by every seat.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct RuntimeSection {
    /// Stable deployment identity used to derive `OpenHuman` agent ids.
    /// Change it to start fresh transcripts and agent homes.
    pub(crate) deployment_id: String,
    /// Logical memory root shared by this deployment; absent derives
    /// `deployment:<deployment_id>`.
    pub(crate) memory_root: Option<String>,
    /// Provider connection.
    pub(crate) provider: Provider,
    /// Default model settings.
    pub(crate) model: Model,
    /// Workspace directory; absent creates an ephemeral workspace.
    pub(crate) workspace: Option<String>,
    /// Session store selector (`memory`, `sqlite`, or `host`).
    pub(crate) session_store: String,
    /// Memory engine selector (`disabled`, `memory`, or `host`).
    pub(crate) memory_engine: String,
    /// Optional named authority baseline.
    pub(crate) permission_profile: Option<String>,
    /// Baseline confinement.
    pub(crate) sandbox: Sandbox,
    /// Global tool rules.
    pub(crate) tool_rules: Option<tinytools::ToolRules>,
    /// Baseline MCP connections.
    pub(crate) mcp: Vec<McpServer>,
    /// Enabled skill directories.
    pub(crate) skills: Vec<String>,
    /// Autonomy tier.
    pub(crate) autonomy: Access,
    /// Runtime limit defaults.
    pub(crate) limits: Limits,
    /// Global coordinator-turn bound across hives. Native cron has a separate
    /// scheduler ceiling with this value; direct turns bypass coordinator admission.
    pub(crate) concurrency: usize,
    /// Include installed user skills in the runtime.
    pub(crate) include_user_skills: bool,
    /// Maximum seats; omission leaves the deployment unbounded.
    pub(crate) max_agents: Option<usize>,
    /// Start scheduler services after deployment construction.
    pub(crate) start_scheduler: bool,
}
impl Default for RuntimeSection {
    fn default() -> Self {
        Self {
            deployment_id: "default".into(),
            memory_root: None,
            provider: Provider::default(),
            model: Model::default(),
            workspace: None,
            session_store: "memory".into(),
            memory_engine: "disabled".into(),
            permission_profile: None,
            sandbox: Sandbox::None,
            tool_rules: None,
            mcp: vec![],
            skills: vec![],
            autonomy: Access::Supervised,
            limits: Limits::default(),
            concurrency: 4,
            include_user_skills: false,
            max_agents: None,
            start_scheduler: false,
        }
    }
}
/// Reusable agent template.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Profile {
    /// Unique template id.
    pub(crate) id: String,
    /// Optional builtin definition name.
    pub(crate) definition_base: Option<String>,
    /// Replace the entire composed system prompt when true.
    pub(crate) bare_prompt: bool,
    /// Override user-skill discovery.
    pub(crate) include_user_skills: Option<bool>,
    /// Fixed system prompt body.
    pub(crate) system_prompt: String,
    /// Named Markdown context, in prompt composition order.
    pub(crate) context: Vec<String>,
    /// Model/sampling overrides.
    pub(crate) model: Model,
    /// Optional authority restriction.
    pub(crate) permission_profile: Option<String>,
    /// Tool visibility.
    pub(crate) tool_scope: ToolScope,
    /// Additional denied tool names.
    pub(crate) deny_tools: Vec<String>,
    /// Additional immutable MCP servers.
    pub(crate) mcp: Vec<McpServer>,
    /// Additional enabled skills.
    pub(crate) skills: Vec<String>,
    /// Allowed subagent template ids.
    pub(crate) subagents: Vec<String>,
    /// Fixed memory binding override.
    pub(crate) memory: Option<Memory>,
    /// Turn limits.
    pub(crate) limits: Limits,
    /// Harness tool group modes, by group id.
    pub(crate) groups: BTreeMap<String, tinyhivemind_lang::GroupMode>,
}
/// Typed dynamic or configured seat overrides; credential fields are absent.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct SeatOverrides {
    /// Replacement fixed system prompt body.
    pub(crate) system_prompt: Option<String>,
    /// Additional context references.
    pub(crate) context: Vec<String>,
    /// Model/sampling overrides.
    pub(crate) model: Model,
    /// Optional authority restriction.
    pub(crate) permission_profile: Option<String>,
    /// Optional tool visibility restriction.
    pub(crate) tool_scope: Option<ToolScope>,
    /// Additional denied tool names.
    pub(crate) deny_tools: Vec<String>,
    /// Additional immutable MCP connections.
    pub(crate) mcp: Vec<McpServer>,
    /// Additional skills.
    pub(crate) skills: Vec<String>,
    /// Allowed subagent template ids; omission inherits.
    pub(crate) subagents: Option<Vec<String>>,
    /// Fixed memory binding override.
    pub(crate) memory: Option<Memory>,
    /// Turn limit overrides.
    pub(crate) limits: Limits,
}
/// One persistent agent/session identity.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Seat {
    /// OpenHuman-compatible agent id.
    pub(crate) id: String,
    /// Optional `OpenHuman` agent id. When absent, deployment lowers this to
    /// `<deployment_id>-<seat_id>` while coordinator configuration keeps the
    /// stable seat id.
    pub(crate) agent_id: Option<String>,
    /// Reusable profile reference.
    pub(crate) profile: String,
    /// Typed final seat settings.
    pub(crate) overrides: SeatOverrides,
}
impl Seat {
    /// `OpenHuman` agent id used for sessions, transcripts and the workspace
    /// profile, leaving [`Seat::id`] as the stable coordinator identity.
    #[must_use]
    pub(crate) fn resolved_agent_id(&self, deployment_id: &str) -> String {
        self.agent_id
            .clone()
            .unwrap_or_else(|| format!("{deployment_id}-{}", self.id))
    }
}
/// Per-hive captured overlay; cannot rebind memory or credentials.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Membership {
    /// Existing seat id.
    pub(crate) seat: String,
    /// Freeform nonempty routing/delegation role.
    pub(crate) role: String,
    /// Additional context references.
    pub(crate) context: Vec<String>,
    /// Optional further authority restriction.
    pub(crate) permission_profile: Option<String>,
    /// Optional assertion of the immutable seat MCP set.
    pub(crate) mcp: Option<Vec<McpServer>>,
}
/// Hive membership and native policy values.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct HiveSpec {
    /// Unique hive identity.
    pub(crate) id: String,
    /// Display name; empty falls back to id.
    pub(crate) name: String,
    /// Members, in declaration order.
    pub(crate) members: Vec<Membership>,
    /// Shared recall namespace; does not rebind seats.
    pub(crate) memory_root: Option<String>,
    /// Pure deliberation episode policy.
    pub(crate) episode: EpisodePolicy,
    /// Semantic completion routing policy.
    pub(crate) routing: RoutingPolicy,
    /// Per-hive scheduling settings.
    pub(crate) coordinator: CoordinatorOptions,
    /// Pure deliberation task division policy.
    pub(crate) division: DivisionPolicy,
    /// Completion turn walls.
    pub(crate) conduct: ConductPolicy,
}
impl Default for HiveSpec {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            members: Vec::new(),
            memory_root: None,
            episode: EpisodePolicy {
                round_width: 1,
                ..EpisodePolicy::default()
            },
            routing: RoutingPolicy {
                minimum_confidence: tinyhivemind_core::responder::Probability::new(600_000)
                    .unwrap_or(tinyhivemind_core::responder::Probability::ONE),
                high_impact_minimum_confidence: tinyhivemind_core::responder::Probability::new(
                    800_000,
                )
                .unwrap_or(tinyhivemind_core::responder::Probability::ONE),
                clarification_threshold: tinyhivemind_core::responder::Probability::new(700_000)
                    .unwrap_or(tinyhivemind_core::responder::Probability::ONE),
                high_impact_threshold: tinyhivemind_core::responder::Probability::new(700_000)
                    .unwrap_or(tinyhivemind_core::responder::Probability::ONE),
                round_width: 4,
                choice_option_limit: 8,
            },
            coordinator: CoordinatorOptions::default(),
            division: DivisionPolicy::default(),
            conduct: ConductPolicy::default(),
        }
    }
}
/// Explicit workflow dispatch target.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum WorkflowTarget {
    /// Native isolated agent cron session.
    Seat(String),
    /// Authorized coordinator hive message.
    Hive(String),
}
/// Scheduled named prompt; services start separately.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Workflow {
    /// Unique job id.
    pub(crate) id: String,
    /// Cron expression, parsed before runtime construction.
    pub(crate) schedule: String,
    /// Authenticated deployment target.
    pub(crate) target: WorkflowTarget,
    /// Prompt submitted when the job runs.
    pub(crate) prompt: String,
    /// Whether this job is enabled.
    #[serde(default = "enabled")]
    pub(crate) enabled: bool,
    /// Maximum retry count.
    #[serde(default)]
    pub(crate) retries: u32,
    /// At most one invocation at once.
    #[serde(default = "enabled")]
    pub(crate) single_flight: bool,
}
fn enabled() -> bool {
    true
}
/// Canonical configuration; parsing has no runtime or secret side effects.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct HiveConfig {
    /// One runtime's defaults.
    pub(crate) runtime: RuntimeSection,
    /// Reusable agent templates.
    pub(crate) profiles: Vec<Profile>,
    /// Named baseline and restriction policies.
    pub(crate) permission_profiles: Vec<PermissionProfile>,
    /// Persistent seats.
    pub(crate) seats: Vec<Seat>,
    /// Hives sharing the seats.
    pub(crate) hives: Vec<HiveSpec>,
    /// Named cron workflows.
    pub(crate) workflows: Vec<Workflow>,
    /// Named Markdown context bodies, resolved by directory loading.
    pub(crate) contexts: BTreeMap<String, String>,
}
/// Manifest that passed all referential and authority checks.
#[derive(Clone, Debug)]
pub(crate) struct ValidatedHiveConfig(pub(super) HiveConfig);
impl ValidatedHiveConfig {
    /// Immutable validated manifest.
    #[must_use]
    pub(crate) fn config(&self) -> &HiveConfig {
        &self.0
    }
    /// Consume validation proof to recover an editable manifest.
    #[must_use]
    pub(crate) fn into_config(self) -> HiveConfig {
        self.0
    }
}
