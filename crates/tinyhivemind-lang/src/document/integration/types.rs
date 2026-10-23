//! Wire types for what a hive's agents run with: runtime, providers, models,
//! MCP servers, skills, toolpacks, permissions, profiles, hooks and workflows.
//!
//! Each mirrors a setting the OpenHuman harness can run, in the language's own
//! vocabulary; none names a harness type. Every struct denies unknown fields
//! and every optional field defaults, so a manifest names only what it sets.
use super::ExecTool;
use crate::ToolAccess;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Where a credential lives; never the credential itself.
///
/// Written `{"env": "NAME"}` or `{"store": "key"}`. The host resolves it at
/// deployment; the language only carries it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum SecretRef {
    /// An environment variable of the deploying process.
    Env(String),
    /// A key in the host's secret store.
    Store(String),
}

/// Turn resource limits; an omitted field inherits the enclosing default.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    /// Wall-clock timeout of one turn in milliseconds; nonzero.
    pub timeout_ms: Option<u64>,
    /// Most model/tool iterations in one turn; nonzero.
    pub iterations: Option<usize>,
    /// Most model spend of one turn in dollars; finite and non-negative.
    pub budget_usd: Option<f64>,
}

impl Limits {
    /// `self` with every unset field taken from `base`.
    #[must_use]
    pub fn or(&self, base: &Self) -> Self {
        Self {
            timeout_ms: self.timeout_ms.or(base.timeout_ms),
            iterations: self.iterations.or(base.iterations),
            budget_usd: self.budget_usd.or(base.budget_usd),
        }
    }
}

/// How much a seat may change on its own.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Access {
    /// No mutating tools.
    ReadOnly,
    /// Mutation under human supervision.
    #[default]
    Supervised,
    /// Mutation subject only to the other gates.
    Autonomous,
}

/// File and shell confinement.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Sandbox {
    /// Access and path rules only.
    #[default]
    None,
    /// No writes and no execution.
    ReadOnly,
    /// The host's jail or container backend.
    Sandboxed,
}

/// Where conversation sessions are stored.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStore {
    /// In process; gone on restart.
    #[default]
    Memory,
    /// SQLite in the workspace.
    Sqlite,
    /// A store the host supplies.
    Host,
}

/// Which memory engine backs seat memory.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryEngine {
    /// No memory engine.
    #[default]
    Disabled,
    /// An in-process engine.
    Memory,
    /// An engine the host supplies.
    Host,
}

/// The runtime every seat of the hive runs on, and the defaults they inherit.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct RuntimeSettings {
    /// Stable deployment identity agent ids derive from; `None` uses the
    /// hive id. Change it to start fresh transcripts and agent homes.
    pub deployment_id: Option<String>,
    /// Workspace directory; `None` is an ephemeral workspace.
    pub workspace: Option<String>,
    /// Session storage.
    pub session_store: SessionStore,
    /// Memory engine.
    pub memory_engine: MemoryEngine,
    /// Autonomy tier of the runtime.
    pub autonomy: Access,
    /// Baseline confinement.
    pub sandbox: Sandbox,
    /// Most coordinator turns running at once across the deployment; nonzero.
    pub concurrency: usize,
    /// Most seats the runtime may hold; `None` is unbounded.
    pub max_agents: Option<usize>,
    /// Also expose the operator's installed skills.
    pub include_user_skills: bool,
    /// Start the scheduler so `workflows` run.
    pub start_scheduler: bool,
    /// Default provider id for every seat.
    pub provider: Option<String>,
    /// Default model id for every seat.
    pub model: Option<String>,
    /// Baseline permission set id.
    pub permissions: Option<String>,
    /// Toolpack ids every seat receives.
    pub toolpacks: Vec<String>,
    /// Default turn limits.
    pub limits: Limits,
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        Self {
            deployment_id: None,
            workspace: None,
            session_store: SessionStore::Memory,
            memory_engine: MemoryEngine::Disabled,
            autonomy: Access::Supervised,
            sandbox: Sandbox::None,
            concurrency: 4,
            max_agents: None,
            include_user_skills: false,
            start_scheduler: false,
            provider: None,
            model: None,
            permissions: None,
            toolpacks: Vec::new(),
            limits: Limits::default(),
        }
    }
}

/// The kind of inference endpoint a provider is.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// Whatever provider the runtime already has.
    #[default]
    Inherit,
    /// An OpenAI-compatible endpoint.
    Openai,
    /// OpenRouter.
    Openrouter,
    /// Anthropic; the host must supply the client.
    Anthropic,
}

/// One inference provider.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Provider {
    /// Identity models and the runtime refer to.
    pub id: String,
    /// Endpoint kind.
    #[serde(default)]
    pub kind: ProviderKind,
    /// Endpoint URL; `None` uses the kind's default.
    #[serde(default)]
    pub endpoint: Option<String>,
    /// API key reference.
    #[serde(default)]
    pub credential: Option<SecretRef>,
}

/// One model a seat may run on.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Model {
    /// Identity seats, profiles and the runtime refer to.
    pub id: String,
    /// Provider id; `None` uses the runtime's provider.
    #[serde(default)]
    pub provider: Option<String>,
    /// The provider's model name.
    pub name: String,
    /// Sampling temperature; finite and non-negative.
    #[serde(default)]
    pub temperature: Option<f64>,
    /// Most generated tokens per model request; nonzero.
    #[serde(default)]
    pub max_tokens: Option<u32>,
    /// Turn limits this model brings.
    #[serde(default)]
    pub limits: Limits,
}

/// How an MCP server is reached.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    /// A child process speaking over stdio; `endpoint` is the program.
    #[default]
    Stdio,
    /// Streamable HTTP; `endpoint` is the URL.
    Http,
}

/// One MCP server connection, fixed when an agent is created.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct McpServer {
    /// Connection name, unique in the package.
    pub id: String,
    /// Transport.
    #[serde(default)]
    pub transport: McpTransport,
    /// Program for stdio, URL for HTTP.
    pub endpoint: String,
    /// Program arguments for stdio.
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment for stdio, every value a reference.
    #[serde(default)]
    pub env: BTreeMap<String, SecretRef>,
    /// HTTP headers, every value a reference.
    #[serde(default)]
    pub headers: BTreeMap<String, SecretRef>,
    /// `None` exposes every server tool; a list limits them.
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    /// Server tools always denied.
    #[serde(default)]
    pub deny_tools: Vec<String>,
}

/// One skill bundle shipped in the package at `skills/<id>/`.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Skill {
    /// The bundle directory name; `skills/<id>/SKILL.md` must exist.
    pub id: String,
}

/// How one harness tool group reaches the model.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GroupMode {
    /// Schemas sent on every model call.
    Advertised,
    /// Callable, but found through the harness's skill index.
    #[default]
    Withheld,
    /// Not registered at all.
    Off,
}

/// A named bundle of tools, MCP servers, skills and harness tool groups.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Toolpack {
    /// Identity seats, profiles and the runtime refer to.
    pub id: String,
    /// What the pack is for.
    #[serde(default)]
    pub description: String,
    /// Executable tool ids.
    #[serde(default)]
    pub tools: Vec<String>,
    /// MCP server ids.
    #[serde(default)]
    pub mcp: Vec<String>,
    /// Skill ids.
    #[serde(default)]
    pub skills: Vec<String>,
    /// Harness tool group id to its mode; the host rejects an unknown group.
    #[serde(default)]
    pub groups: BTreeMap<String, GroupMode>,
}

/// A named authority baseline or restriction.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PermissionSet {
    /// Identity seats, profiles and the runtime refer to.
    pub id: String,
    /// Access tier.
    #[serde(default)]
    pub access: Access,
    /// Confinement.
    #[serde(default)]
    pub sandbox: Sandbox,
    /// `None` leaves tool names unrestricted; a list is the only ones allowed.
    #[serde(default)]
    pub allow_tools: Option<Vec<String>>,
    /// Tool names always denied.
    #[serde(default)]
    pub deny_tools: Vec<String>,
    /// Allowed message destinations; `None` is unrestricted.
    #[serde(default)]
    pub send_destinations: Option<Vec<String>>,
    /// Allowed send operations; `None` is unrestricted.
    #[serde(default)]
    pub send_operations: Option<Vec<String>>,
    /// Argument-sensitive tool rules in the harness's own form, checked when
    /// the host deploys; `null` inherits.
    #[serde(default)]
    pub tool_rules: serde_json::Value,
}

/// The integration a seat or profile runs with.
///
/// The same fields appear on a [`crate::Seat`]; a profile is a reusable set
/// of them that seats name.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Identity seats refer to.
    pub id: String,
    /// Model id.
    #[serde(default)]
    pub model: Option<String>,
    /// Permission set id.
    #[serde(default)]
    pub permissions: Option<String>,
    /// Toolpack ids.
    #[serde(default)]
    pub toolpacks: Vec<String>,
    /// Tool names allowed and denied.
    #[serde(default)]
    pub tools: ToolAccess,
    /// MCP server ids.
    #[serde(default)]
    pub mcp: Vec<String>,
    /// Skill ids.
    #[serde(default)]
    pub skills: Vec<String>,
    /// Hook ids.
    #[serde(default)]
    pub hooks: Vec<String>,
    /// `context/*.md` paths composed into the prompt.
    #[serde(default)]
    pub context: Vec<String>,
    /// Turn limits.
    #[serde(default)]
    pub limits: Limits,
    /// Profile ids a seat using this profile may delegate to.
    #[serde(default)]
    pub subagents: Vec<String>,
    /// Use the prompt alone, without the harness's composed system prompt.
    #[serde(default)]
    pub bare_prompt: bool,
    /// A harness built-in agent definition to start from.
    #[serde(default)]
    pub definition_base: Option<String>,
}

/// The event a hook runs on.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HookEvent {
    /// Before a tool call; the hook may refuse it.
    #[default]
    PreToolUse,
    /// After a tool call.
    PostToolUse,
    /// When a turn ends; the hook may ask for one more turn.
    Stop,
}

/// An executable tool bound to an event.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Hook {
    /// Identity seats and profiles refer to.
    pub id: String,
    /// The event.
    #[serde(default)]
    pub event: HookEvent,
    /// Tool names a tool event applies to (`*` wildcards); `None` is every
    /// tool. Not allowed on `stop`.
    #[serde(default)]
    pub matcher: Option<String>,
    /// The executable tool the hook runs.
    pub run: String,
    /// Treat a failing hook as a refusal.
    #[serde(default)]
    pub fail_closed: bool,
    /// Wall clock for the hook; nonzero. `None` uses the tool's timeout.
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// Agent session settings: how much conversation a seat keeps and sees.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct SessionSettings {
    /// Compact the transcript when it grows past the model's window.
    pub compact_context: Option<bool>,
    /// Most history messages sent per model call; nonzero.
    pub max_history_messages: Option<usize>,
    /// Largest tool result kept in the transcript, in bytes; nonzero.
    pub tool_result_budget_bytes: Option<usize>,
    /// Most tool iterations per turn; nonzero.
    pub max_tool_iterations: Option<usize>,
    /// Run independent tool calls of one step in parallel.
    pub parallel_tools: Option<bool>,
    /// Compose `AGENTS.md` from the workspace into the prompt.
    pub agents_md: Option<bool>,
}

/// The ranker that answers a seat's search over tools it was not sent.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JevRanker {
    /// Jev, falling back to BM25 when it is unreachable.
    #[default]
    Jev,
    /// Jev when a credential is present, BM25 otherwise.
    Auto,
    /// Lexical BM25 only; no network.
    Bm25,
    /// Serve Jev and record BM25 beside it.
    Compare,
}

/// Which route Jev decision calls take.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum JevRoute {
    /// The first route the process has a credential for.
    #[default]
    Auto,
    /// The TinyHumans proxy.
    Tinyhumans,
    /// TypeSafe's API.
    Typesafe,
    /// OpenRouter's System One API.
    Openrouter,
}

/// Jev, the decision model the harness ranks deferred tools with.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub struct JevSettings {
    /// The ranker.
    pub ranker: JevRanker,
    /// The route.
    pub route: JevRoute,
    /// API origin replacing the `typesafe` or `openrouter` route's.
    pub base_url: Option<String>,
    /// Matches a search returns by default; nonzero.
    pub top_k: usize,
}

impl Default for JevSettings {
    fn default() -> Self {
        Self {
            ranker: JevRanker::Jev,
            route: JevRoute::Auto,
            base_url: None,
            top_k: 3,
        }
    }
}

/// What a workflow submits its prompt to.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum WorkflowTarget {
    /// One seat, in an isolated session.
    Seat(String),
    /// The hive, as a host message.
    Hive(String),
}

/// A scheduled prompt.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Workflow {
    /// Identity, unique in the package.
    pub id: String,
    /// Five-field cron expression.
    pub schedule: String,
    /// Where the prompt goes.
    pub target: WorkflowTarget,
    /// The prompt.
    pub prompt: String,
    /// Whether the job runs.
    #[serde(default = "yes")]
    pub enabled: bool,
    /// Retries after a failure.
    #[serde(default)]
    pub retries: u32,
    /// At most one run at a time.
    #[serde(default = "yes")]
    pub single_flight: bool,
}

fn yes() -> bool {
    true
}

/// Everything a hive's agents run with, as declared in `hive.json`.
///
/// Flattened into [`crate::Manifest`]: each field is a top-level section.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Integrations {
    /// The runtime and the defaults every seat inherits.
    pub runtime: RuntimeSettings,
    /// Inference providers.
    pub providers: Vec<Provider>,
    /// Models.
    pub models: Vec<Model>,
    /// MCP servers.
    pub mcp: Vec<McpServer>,
    /// Skill bundles under `skills/`.
    pub skills: Vec<Skill>,
    /// Executable tools.
    pub tools: Vec<ExecTool>,
    /// Named tool bundles.
    pub toolpacks: Vec<Toolpack>,
    /// Permission sets.
    pub permissions: Vec<PermissionSet>,
    /// Reusable seat integrations.
    pub profiles: Vec<Profile>,
    /// Executable tools bound to events.
    pub hooks: Vec<Hook>,
    /// Agent session settings.
    pub session: SessionSettings,
    /// Jev tool-search settings.
    pub jev: JevSettings,
    /// Feature flags.
    pub features: BTreeMap<String, bool>,
    /// Scheduled prompts.
    pub workflows: Vec<Workflow>,
}
