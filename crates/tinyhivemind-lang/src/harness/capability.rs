//! What each harness kind can honour: the capability table of the
//! [harness spec](../../../../docs/specs/harnesses.md#capabilities), as data.
//!
//! `check` asks [`HarnessKind::support`] for every setting a seat resolves to
//! and reports a [`Support::Rejected`] one at the field's location. The
//! process runners ask the same question before they write a harness config,
//! so a setting the table rejects is never accepted and dropped. A row moves
//! only with the evidence its [`Unlock`] names.
use super::HarnessKind;
use std::fmt;

/// One row of the capability table: a setting, or a family of settings a
/// package may ask of a harness.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Capability {
    /// The prompt, a model name, hive and executable tools, and
    /// `limits.timeout_ms`.
    Essentials,
    /// Provider `openai` with an explicit `endpoint` and no `credential`.
    OpenaiEndpoint,
    /// Provider `openai` without an `endpoint`.
    OpenaiDefaultEndpoint,
    /// Provider `openrouter`.
    Openrouter,
    /// Provider `anthropic`.
    Anthropic,
    /// Provider `inherit`, or no provider at all: a child process has none
    /// to inherit.
    InheritedProvider,
    /// A provider `credential`.
    ProviderCredential,
    /// Hooks and permissions on hive and executable tools.
    ServedToolPolicy,
    /// `sandbox` `none` or absent, on `runtime` and permission sets.
    NoSandbox,
    /// `sandbox` `read_only` or `sandboxed`.
    Sandbox,
    /// `autonomy` and `permissions.access`.
    Autonomy,
    /// A built-in tool group turned on (`advertised` or `withheld`).
    BuiltinToolGroups,
    /// `tool_rules` on built-in tools.
    BuiltinToolRules,
    /// Hooks on built-in tools.
    BuiltinToolHooks,
    /// The package's MCP servers.
    McpServers,
    /// `session_store = "host"`.
    HostSessionStore,
    /// `memory_engine = "host"`.
    HostMemoryEngine,
    /// A profile's `subagents`.
    Subagents,
    /// A profile's `definition_base`.
    DefinitionBase,
    /// A `jev` section other than the default.
    Jev,
    /// Any `session` setting.
    SessionTuning,
    /// `workflows`.
    Workflows,
    /// Skills: a seat's, a profile's or a toolpack's.
    Skills,
    /// `runtime.include_user_skills`.
    UserSkills,
    /// `limits.budget_usd`.
    Budget,
    /// `limits.iterations`; OpenHuman's own cap is
    /// [`HarnessKind::iteration_cap`].
    IterationLimit,
    /// A model's `temperature`.
    Temperature,
    /// A model's `max_tokens`.
    MaxTokens,
    /// A profile's `bare_prompt`.
    BarePrompt,
    /// Tool allow and deny lists: `tools` on a seat or profile, and a
    /// permission set's `allow_tools` and `deny_tools`.
    ToolLists,
    /// A permission set's `send_destinations` and `send_operations`.
    SendPolicy,
    /// A `stop` hook.
    StopHooks,
    /// A seat's factory `config`.
    FactoryConfig,
    /// A seat's harness `agent_id`.
    AgentId,
    /// `runtime.workspace`.
    Workspace,
    /// `runtime.deployment_id`.
    DeploymentId,
    /// `runtime.concurrency` other than its default.
    Concurrency,
    /// `runtime.max_agents`.
    MaxAgents,
    /// `session_store = "sqlite"`.
    SqliteSessions,
    /// `memory_engine = "memory"`.
    InProcessMemory,
}

/// How one kind honours one [`Capability`].
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Support {
    /// The harness does it.
    Harness,
    /// The host does it, outside the harness: through its MCP server, its
    /// approver or the coordinator's schedule.
    Host,
    /// `check` rejects it until the evidence named moves the row.
    Rejected(Unlock),
}

/// What it takes to move a rejected row.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Unlock {
    /// Only a conformance test on the pinned binary, which shows the setting
    /// reaches the harness and is enforced.
    Conformance,
    /// A harness release that has the setting, then a conformance test.
    Release,
    /// A change of the spec, and usually host work: the spec rejects the
    /// setting on this path, or does not decide it, so no test alone moves
    /// it.
    Spec,
}

/// The model API a kind speaks to an `openai` provider's endpoint.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ModelApi {
    /// `POST /chat/completions`.
    ChatCompletions,
    /// `POST /responses`.
    Responses,
}

impl Support {
    /// Whether a package may ask for it.
    #[must_use]
    pub const fn is_supported(self) -> bool {
        !matches!(self, Self::Rejected(_))
    }
}

impl Capability {
    /// Every capability, in the order of the spec's table.
    pub const ALL: [Self; 40] = [
        Self::Essentials,
        Self::OpenaiEndpoint,
        Self::OpenaiDefaultEndpoint,
        Self::Openrouter,
        Self::Anthropic,
        Self::InheritedProvider,
        Self::ProviderCredential,
        Self::ServedToolPolicy,
        Self::NoSandbox,
        Self::Sandbox,
        Self::Autonomy,
        Self::BuiltinToolGroups,
        Self::BuiltinToolRules,
        Self::BuiltinToolHooks,
        Self::McpServers,
        Self::HostSessionStore,
        Self::HostMemoryEngine,
        Self::Subagents,
        Self::DefinitionBase,
        Self::Jev,
        Self::SessionTuning,
        Self::Workflows,
        Self::Skills,
        Self::UserSkills,
        Self::Budget,
        Self::IterationLimit,
        Self::Temperature,
        Self::MaxTokens,
        Self::BarePrompt,
        Self::ToolLists,
        Self::SendPolicy,
        Self::StopHooks,
        Self::FactoryConfig,
        Self::AgentId,
        Self::Workspace,
        Self::DeploymentId,
        Self::Concurrency,
        Self::MaxAgents,
        Self::SqliteSessions,
        Self::InProcessMemory,
    ];

    /// The row: support for `openhuman`, `openclaw` and `codex`, in that
    /// order ([`HarnessKind::ALL`]).
    const fn row(self) -> [Support; 3] {
        use Support::{Harness as H, Host};
        const T: Support = Support::Rejected(Unlock::Conformance);
        const R: Support = Support::Rejected(Unlock::Release);
        const D: Support = Support::Rejected(Unlock::Spec);
        match self {
            Self::Essentials | Self::OpenaiEndpoint | Self::NoSandbox => [H, H, H],
            Self::ServedToolPolicy | Self::Autonomy | Self::McpServers | Self::Workflows => {
                [Host, Host, Host]
            }
            // The spike ran one provider shape per kind; every other one
            // waits for a conformance test, `credential` for one whose
            // endpoint refuses a call without the right key.
            Self::OpenaiDefaultEndpoint
            | Self::Openrouter
            | Self::Anthropic
            | Self::ProviderCredential => [T, T, T],
            // No fallback: a process harness never uses its own default.
            Self::InheritedProvider
            // Executable tools run from the host's MCP server, where nothing
            // confines them, and a harness sandbox does not reach them.
            | Self::Sandbox
            // Built-in tools stay off until their permissions are enforced
            // and the instance is contained, so nothing can hook them.
            | Self::BuiltinToolGroups
            | Self::BuiltinToolHooks
            | Self::HostSessionStore
            | Self::HostMemoryEngine => [D, D, D],
            Self::BuiltinToolRules => [R, D, D],
            // The spec's table does not decide these, so they start
            // rejected: a setting nothing on the process path enforces must
            // not be accepted. The tool lists and send policy are enforced
            // by the linked adapter's permission gate, not by the host.
            Self::Skills
            | Self::UserSkills
            | Self::Budget
            | Self::Temperature
            | Self::MaxTokens
            | Self::BarePrompt
            | Self::ToolLists
            | Self::SendPolicy
            | Self::StopHooks
            | Self::FactoryConfig
            | Self::AgentId
            | Self::Workspace
            | Self::DeploymentId
            | Self::Concurrency
            | Self::MaxAgents
            | Self::SqliteSessions
            | Self::InProcessMemory => [D, D, D],
            // OpenHuman honours a limit up to its fixed cap.
            Self::IterationLimit => [H, D, D],
            Self::Subagents | Self::DefinitionBase | Self::Jev | Self::SessionTuning => [T, D, D],
        }
    }
}

impl HarnessKind {
    /// How this kind honours `capability`.
    #[must_use]
    pub const fn support(self, capability: Capability) -> Support {
        let row = capability.row();
        match self {
            Self::OpenHuman => row[0],
            Self::OpenClaw => row[1],
            Self::Codex => row[2],
        }
    }

    /// The API this kind speaks to an `openai` provider. The language cannot
    /// tell which one an endpoint serves, so `check` cannot see a mismatch;
    /// the turn fails instead.
    #[must_use]
    pub const fn model_api(self) -> ModelApi {
        match self {
            Self::OpenHuman | Self::OpenClaw => ModelApi::ChatCompletions,
            Self::Codex => ModelApi::Responses,
        }
    }

    /// The model iterations a turn stops at, when the released harness fixes
    /// them: `OpenHuman` ends a custom agent after eight and reports a normal
    /// end, so a package asking for more is rejected until it is
    /// configurable.
    #[must_use]
    pub const fn iteration_cap(self) -> Option<usize> {
        match self {
            Self::OpenHuman => Some(8),
            Self::OpenClaw | Self::Codex => None,
        }
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Essentials => "the prompt, model name, hive and executable tools and timeout",
            Self::OpenaiEndpoint => "provider openai with an endpoint and no credential",
            Self::OpenaiDefaultEndpoint => "provider openai without an endpoint",
            Self::Openrouter => "provider openrouter",
            Self::Anthropic => "provider anthropic",
            Self::InheritedProvider => "an inherited provider, or none",
            Self::ProviderCredential => "a provider credential",
            Self::ServedToolPolicy => "hooks and permissions on hive and executable tools",
            Self::NoSandbox => "no sandbox",
            Self::Sandbox => "a sandbox",
            Self::Autonomy => "autonomy and access",
            Self::BuiltinToolGroups => "a built-in tool group turned on",
            Self::BuiltinToolRules => "tool rules on built-in tools",
            Self::BuiltinToolHooks => "hooks on built-in tools",
            Self::McpServers => "package mcp servers",
            Self::HostSessionStore => "a host session store",
            Self::HostMemoryEngine => "a host memory engine",
            Self::Subagents => "subagents",
            Self::DefinitionBase => "a definition base",
            Self::Jev => "jev settings",
            Self::SessionTuning => "session tuning",
            Self::Workflows => "workflows",
            Self::Skills => "skills",
            Self::UserSkills => "the operator's own skills",
            Self::Budget => "a turn budget",
            Self::IterationLimit => "an iteration limit",
            Self::Temperature => "a model temperature",
            Self::MaxTokens => "a model token limit",
            Self::BarePrompt => "a bare prompt",
            Self::ToolLists => "tool allow and deny lists",
            Self::SendPolicy => "send destination and operation limits",
            Self::StopHooks => "stop hooks",
            Self::FactoryConfig => "a factory config",
            Self::AgentId => "a harness agent id",
            Self::Workspace => "a runtime workspace",
            Self::DeploymentId => "a deployment id",
            Self::Concurrency => "a runtime concurrency",
            Self::MaxAgents => "an agent limit",
            Self::SqliteSessions => "sqlite sessions",
            Self::InProcessMemory => "an in-process memory engine",
        })
    }
}

impl fmt::Display for Unlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Conformance => "rejected until a conformance test on the pinned binary proves it",
            Self::Release => {
                "rejected until a harness release has it and a conformance test proves it"
            }
            Self::Spec => "rejected by the harness spec",
        })
    }
}
