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
    /// A change of the spec and host work: the setting cannot be honoured
    /// on this path as designed, so no test alone moves it.
    Decision,
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
    pub const ALL: [Self; 22] = [
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
    ];

    /// The row: support for `openhuman`, `openclaw` and `codex`, in that
    /// order ([`HarnessKind::ALL`]).
    const fn row(self) -> [Support; 3] {
        use Support::{Harness as H, Host};
        const T: Support = Support::Rejected(Unlock::Conformance);
        const R: Support = Support::Rejected(Unlock::Release);
        const D: Support = Support::Rejected(Unlock::Decision);
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
    /// them: OpenHuman ends a custom agent after eight and reports a normal
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
            Self::Decision => "rejected by the harness spec",
        })
    }
}
