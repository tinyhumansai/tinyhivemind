//! Every field of every integration type, and the capability that decides
//! it: the default-deny half of the capability table.
//!
//! When a package is judged, a field set to anything but its default is
//! reported unless its kind supports the field's [`Capability`]. A field the
//! spec's table does not list has a row that starts rejected, so nothing a
//! package says is accepted and left unenforced. A test serializes each type
//! and fails when a field has no entry here, so a field added later must be
//! decided before it can be written.
use super::Capability as C;

/// How one field is judged.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Field {
    /// Language structure with no setting of its own: an identity, a
    /// reference whose target is judged where it is declared, prose, or a
    /// coordinator policy the coordinator runs whatever the harness.
    Structure,
    /// An object judged field by field under another owner.
    Nested(&'static str),
    /// Any value but the default asks for this capability.
    Judged(C),
    /// Judged by value in `validation/capability.rs` (a provider's shape, a
    /// sandbox, a store, tool rules, tool groups, a hook's event, the
    /// iteration limit).
    Special,
}

use Field::{Judged as J, Nested as N, Special as X, Structure as S};

/// Each owner's fields, in wire order.
const FIELDS: &[(&str, &[(&str, Field)])] = &[
    (
        "manifest",
        &[
            // The hive's structure and deliberation are the coordinator's.
            ("$schema", S),
            ("version", S),
            ("id", S),
            ("name", S),
            ("roles", S),
            ("seats", S),
            ("desks", S),
            ("goals", S),
            ("missions", S),
            ("topology", S),
            ("referrals", S),
            ("context", J(C::Essentials)),
            ("policies", S),
            ("coordinator", S),
            // Sections are judged entry by entry, where a seat reaches them.
            ("runtime", S),
            ("providers", S),
            ("models", S),
            ("mcp", S),
            ("skills", S),
            ("tools", S),
            ("toolpacks", S),
            ("permissions", S),
            ("profiles", S),
            ("hooks", S),
            ("session", S),
            ("jev", S),
            // Switches empty an integration in lowering; every flag reaches
            // executable tools as `THM_FEATURE_*` through `tinyhivemind-exec`.
            ("features", J(C::Essentials)),
            ("workflows", J(C::Workflows)),
            ("harnesses", S),
        ],
    ),
    (
        "runtime",
        &[
            ("deployment_id", J(C::DeploymentId)),
            ("workspace", J(C::Workspace)),
            ("session_store", X),
            ("memory_engine", X),
            ("autonomy", J(C::Autonomy)),
            ("sandbox", X),
            ("concurrency", J(C::Concurrency)),
            ("max_agents", J(C::MaxAgents)),
            ("include_user_skills", J(C::UserSkills)),
            ("start_scheduler", J(C::Workflows)),
            ("provider", S),
            ("model", S),
            ("permissions", S),
            ("toolpacks", S),
            ("limits", N("limits")),
            ("harness", S),
        ],
    ),
    (
        "limits",
        &[
            ("timeout_ms", J(C::Essentials)),
            ("iterations", X),
            ("budget_usd", J(C::Budget)),
        ],
    ),
    (
        "provider",
        &[("id", S), ("kind", X), ("endpoint", X), ("credential", X)],
    ),
    (
        "model",
        &[
            ("id", S),
            ("provider", S),
            ("name", J(C::Essentials)),
            ("temperature", J(C::Temperature)),
            ("max_tokens", J(C::MaxTokens)),
            ("limits", N("limits")),
        ],
    ),
    (
        "mcp",
        &[
            ("id", S),
            ("transport", J(C::McpServers)),
            ("endpoint", J(C::McpServers)),
            ("args", J(C::McpServers)),
            ("env", J(C::McpServers)),
            ("headers", J(C::McpServers)),
            ("tools", J(C::McpServers)),
            ("deny_tools", J(C::McpServers)),
        ],
    ),
    ("skill", &[("id", J(C::Skills))]),
    (
        "tool",
        &[
            ("id", S),
            ("description", J(C::Essentials)),
            ("command", J(C::Essentials)),
            ("args", X),
            ("argv", J(C::Essentials)),
            ("stdin", J(C::Essentials)),
            ("output", J(C::Essentials)),
            ("timeout_ms", J(C::Essentials)),
            ("env", J(C::Essentials)),
            ("side_effects", J(C::Essentials)),
            ("cwd", J(C::Essentials)),
        ],
    ),
    (
        "toolpack",
        &[
            ("id", S),
            ("description", S),
            ("tools", J(C::Essentials)),
            ("mcp", J(C::McpServers)),
            ("skills", J(C::Skills)),
            ("groups", X),
        ],
    ),
    (
        "permissions",
        &[
            ("id", S),
            ("access", J(C::Autonomy)),
            ("sandbox", X),
            ("allow_tools", J(C::ToolLists)),
            ("deny_tools", J(C::ToolLists)),
            ("send_destinations", J(C::SendPolicy)),
            ("send_operations", J(C::SendPolicy)),
            ("tool_rules", X),
        ],
    ),
    (
        "profile",
        &[
            ("id", S),
            ("model", S),
            ("permissions", S),
            ("toolpacks", S),
            ("tools", N("tools")),
            ("mcp", J(C::McpServers)),
            ("skills", J(C::Skills)),
            ("hooks", S),
            ("context", J(C::Essentials)),
            ("limits", N("limits")),
            ("subagents", J(C::Subagents)),
            ("bare_prompt", J(C::BarePrompt)),
            ("definition_base", J(C::DefinitionBase)),
            ("harness", S),
        ],
    ),
    (
        "tools",
        &[("allow", J(C::ToolLists)), ("deny", J(C::ToolLists))],
    ),
    (
        "hook",
        &[
            ("id", S),
            ("event", X),
            ("matcher", J(C::ServedToolPolicy)),
            ("run", J(C::ServedToolPolicy)),
            ("fail_closed", J(C::ServedToolPolicy)),
            ("timeout_ms", J(C::ServedToolPolicy)),
        ],
    ),
    (
        "session",
        &[
            ("compact_context", J(C::SessionTuning)),
            ("max_history_messages", J(C::SessionTuning)),
            ("tool_result_budget_bytes", J(C::SessionTuning)),
            ("max_tool_iterations", J(C::SessionTuning)),
            ("parallel_tools", J(C::SessionTuning)),
            ("agents_md", J(C::SessionTuning)),
        ],
    ),
    (
        "jev",
        &[
            ("ranker", J(C::Jev)),
            ("route", J(C::Jev)),
            ("base_url", J(C::Jev)),
            ("top_k", J(C::Jev)),
        ],
    ),
    (
        "workflow",
        &[
            ("id", S),
            ("schedule", J(C::Workflows)),
            ("target", J(C::Workflows)),
            ("prompt", J(C::Workflows)),
            ("enabled", J(C::Workflows)),
            ("retries", J(C::Workflows)),
            ("single_flight", J(C::Workflows)),
        ],
    ),
    (
        "seat",
        &[
            ("id", S),
            ("label", S),
            ("role", S),
            ("prompt", J(C::Essentials)),
            // The host's name for the factory that creates the agent. It
            // carries no setting a harness applies; what the seat runs on
            // is its `harness`, judged on its own.
            ("template", S),
            ("config", J(C::FactoryConfig)),
            ("model", S),
            ("tools", N("tools")),
            ("mcp", J(C::McpServers)),
            ("skills", J(C::Skills)),
            ("profile", S),
            ("agent_id", J(C::AgentId)),
            ("toolpacks", S),
            ("hooks", S),
            ("permissions", S),
            ("limits", N("limits")),
            ("context", J(C::Essentials)),
            ("harness", S),
        ],
    ),
    // The harness entry is the decision itself.
    ("harness", &[("id", S), ("kind", S), ("isolation", S)]),
];

/// The decision for `owner`'s field `key`, if it has one.
pub(crate) fn field(owner: &str, key: &str) -> Option<Field> {
    FIELDS
        .iter()
        .find(|(name, _)| *name == owner)?
        .1
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, field)| *field)
}

#[cfg(test)]
/// Every owner, in table order.
pub(crate) fn owners() -> Vec<&'static str> {
    FIELDS.iter().map(|(owner, _)| *owner).collect()
}

#[cfg(test)]
/// `owner`'s field names, sorted.
pub(crate) fn keys(owner: &str) -> Vec<String> {
    let mut keys: Vec<String> = FIELDS
        .iter()
        .find(|(name, _)| *name == owner)
        .map(|(_, fields)| fields.iter().map(|(key, _)| (*key).to_owned()).collect())
        .unwrap_or_default();
    keys.sort();
    keys
}
