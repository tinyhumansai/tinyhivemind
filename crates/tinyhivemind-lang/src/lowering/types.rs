//! Translate validated packages into existing core values and host requests.
use crate::{
    CoordinatorSettings, ExecTool, Goal, GroupMode, Hook, JevSettings, Limits, Link, McpServer,
    MemorySpec, Mission, Model, PermissionSet, Policies, Provider, RuntimeSettings, SeatMemory,
    SessionSettings, Skill, ToolAccess, Workflow,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tinyhivemind_core::{desk::Desk, embed::RouteCandidate};
/// A complete host-neutral activation snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct LoweredHive {
    /// Stable package identity for host activation and lineage association.
    pub id: String,
    /// Human-readable hive name.
    pub name: String,
    /// Goal decomposition and host-evaluated acceptance criteria.
    pub goals: Vec<Goal>,
    /// Work obligations binding roles to goal identities.
    pub missions: Vec<Mission>,
    /// Directed desk hierarchy; cycles are preserved.
    pub topology: Vec<Link>,
    /// Directed desk referral channels.
    pub referrals: Vec<Link>,
    /// Declared shared context paths and their complete document contents.
    pub context: BTreeMap<String, String>,
    /// Existing desk values.
    pub desks: Vec<Desk>,
    /// Routing candidates with declared roles preserved.
    pub candidates: Vec<RouteCandidate>,
    /// Host seat creation inputs.
    pub seats: Vec<LoweredSeat>,
    /// Existing core policy values.
    pub policies: Policies,
    /// Host-neutral coordinator options.
    pub coordinator: CoordinatorSettings,
    /// Memory identities and bindings, without contents.
    pub memory: MemorySpec,
    /// What the hive's agents run with, hive-wide.
    pub integrations: LoweredIntegrations,
}
/// The hive-wide integration: the runtime and the settings every seat shares.
#[derive(Clone, Debug, PartialEq)]
pub struct LoweredIntegrations {
    /// Runtime settings as declared.
    pub runtime: RuntimeSettings,
    /// The runtime's default provider, resolved.
    pub provider: Option<Provider>,
    /// The runtime's default model, resolved.
    pub model: Option<LoweredModel>,
    /// The runtime's baseline permission set, resolved.
    pub permissions: Option<PermissionSet>,
    /// Every skill bundle the package ships (empty when `skills` is off);
    /// the host installs them from `skills/` and each seat selects its own.
    pub skills: Vec<Skill>,
    /// Agent session settings.
    pub session: SessionSettings,
    /// Jev tool-search settings.
    pub jev: JevSettings,
    /// Feature flags, as declared.
    pub features: BTreeMap<String, bool>,
    /// Scheduled prompts.
    pub workflows: Vec<Workflow>,
}
/// A model with the provider it runs on.
#[derive(Clone, Debug, PartialEq)]
pub struct LoweredModel {
    /// The model as declared.
    pub model: Model,
    /// Its provider: the model's own, else the runtime's; `None` inherits
    /// whatever provider the host runtime has.
    pub provider: Option<Provider>,
}
/// A hook with the executable tool it runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoweredHook {
    /// The hook as declared.
    pub hook: Hook,
    /// The tool it runs.
    pub tool: ExecTool,
}
/// One seat's integration after precedence and toolpack expansion.
///
/// Runtime defaults, then the seat's profile, then the seat: a scalar takes
/// the most specific value, a list concatenates without repeats, limits merge
/// field by field, and `tools.allow` is the most specific non-empty list.
/// Toolpacks expand into their tools, MCP servers, skills and groups, and a
/// feature switched off removes its integration.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoweredSeatIntegration {
    /// The profile the seat starts from.
    pub profile: Option<String>,
    /// The harness agent id the seat declared.
    pub agent_id: Option<String>,
    /// The model the seat runs on.
    pub model: Option<LoweredModel>,
    /// Tool names allowed and denied.
    pub tools: ToolAccess,
    /// MCP servers the seat connects to.
    pub mcp: Vec<McpServer>,
    /// Skills the seat has.
    pub skills: Vec<Skill>,
    /// Executable tools the seat may call.
    pub programs: Vec<ExecTool>,
    /// Hooks on the seat's turns.
    pub hooks: Vec<LoweredHook>,
    /// Harness tool group modes from its toolpacks.
    pub groups: BTreeMap<String, GroupMode>,
    /// Permission sets that all apply, broadest first: runtime, profile,
    /// seat.
    pub permissions: Vec<PermissionSet>,
    /// Turn limits.
    pub limits: Limits,
    /// Extra context documents and their contents, in composition order.
    pub context: Vec<(String, String)>,
    /// Profile ids the seat may delegate to.
    pub subagents: Vec<String>,
    /// Use the prompt alone, without the harness's composed system prompt.
    pub bare_prompt: bool,
    /// A harness built-in definition to start from.
    pub definition_base: Option<String>,
}
/// Resolved identity plus runtime memory configuration retained for the host.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LoweredMemoryBinding {
    /// The namespace's final id: the text after its last `:`, so
    /// `team:hive/agent:critic` yields `critic`.
    pub agent_id: String,
    /// Full resolved namespace retained for host capability checks.
    pub namespace: String,
    /// Agent or shared pool identity.
    pub kind: crate::MemoryKind,
    /// Identity lifecycle, freeze and fork requirements.
    pub identity: crate::MemoryIdentity,
    /// The namespace's parent: the text before its last `/`, so
    /// `team:hive/agent:critic` yields `team:hive`. A namespace with no `/`
    /// yields [`crate::MemorySpec::root`].
    pub root: String,
    /// Recall, remember and read settings the host must install.
    pub settings: SeatMemory,
}
/// Host-owned seat configuration, without a runner or credentials.
#[derive(Clone, Debug, PartialEq)]
pub struct LoweredSeat {
    /// Seat identity.
    pub id: String,
    /// Display label.
    pub label: String,
    /// Host factory template.
    pub template: String,
    /// Host nonsecret configuration.
    pub config: serde_json::Value,
    /// Prompt body, without frontmatter.
    pub prompt: String,
    /// Resolved memory identity and preserved host settings.
    pub memory: Option<LoweredMemoryBinding>,
    /// What the seat runs with, resolved.
    pub integration: LoweredSeatIntegration,
}
