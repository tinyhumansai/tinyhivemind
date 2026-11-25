//! Hive packages as a host deploys them.
//!
//! [`PackageSource`] reads and checks a hive package from disk; [`plan::plan`]
//! lowers checked packages into the deployment plan of [`crate::config`].
mod package;
pub mod plan;
use crate::policy::ManagementRequest;
pub use package::PackageSource;
use tinyhivemind_hives::{CoordinatorOptions, RetentionPolicy};
use tinyhivemind_lang::{CoordinatorSettings, LoweredSeat};

/// Convert portable scheduling settings without changing their bounds.
#[must_use]
pub fn coordinator_options(settings: &CoordinatorSettings) -> CoordinatorOptions {
    CoordinatorOptions {
        round_width: settings.round_width,
        conduct_policy: settings.conduct_policy,
        broadcast_budget: settings.broadcast_budget,
        retention: RetentionPolicy {
            settled_episodes: settings.retention.settled_episodes,
            delivered: settings.retention.delivered,
            interrupted: settings.retention.interrupted,
            pending_per_agent: settings.retention.pending_per_agent,
        },
    }
}

/// Carry a lowered seat and its full memory contract to a host factory.
///
/// `config` retains the original nonsecret settings under `config`, alongside
/// the seat id, prompt, and the resolved model name, tool access, MCP server
/// ids, skill ids and executable tool ids. Factories consuming hive
/// definitions should validate and honor this portable envelope.
#[must_use]
pub fn management_request(seat: &LoweredSeat) -> ManagementRequest {
    let integration = &seat.integration;
    let ids = |items: Vec<&str>| serde_json::json!(items);
    ManagementRequest::CreateAgent {
        template: seat.template.clone(),
        coordinator_id: Some(seat.id.clone()),
        config: serde_json::json!({
            "id": seat.id, "label": seat.label, "config": seat.config,
            "prompt": seat.prompt,
            "model": integration.model.as_ref().map(|model| &model.model.name),
            "tools": integration.tools,
            "mcp": ids(integration.mcp.iter().map(|s| s.id.as_str()).collect()),
            "skills": ids(integration.skills.iter().map(|s| s.id.as_str()).collect()),
            "programs": ids(integration.programs.iter().map(|t| t.id.as_str()).collect()),
        }),
        memory: seat.memory.clone().map(Box::new),
    }
}

#[cfg(test)]
mod test;
