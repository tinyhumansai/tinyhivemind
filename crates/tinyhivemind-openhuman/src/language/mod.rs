//! Explicit conversions of portable definitions at the `OpenHuman` boundary.
use crate::{Error, ManagementRequest, Result};
use openhuman_embed::MemoryBinding;
use tinyhivemind_hives::{CoordinatorOptions, RetentionPolicy};
use tinyhivemind_lang::{CoordinatorSettings, LoweredMemoryBinding, LoweredSeat, MemoryKind};

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

/// Extract the harness binding from a portable agent memory identity.
///
/// This only converts the namespace. The factory must additionally implement
/// the retained recall, reaches, read-only and lifecycle contract before using
/// the binding; the harness binding alone does not represent those settings.
/// # Errors
/// Refuses pool identities, invalid roots or ids, and namespaces that do not
/// map to the harness's `<root>/agent:<id>` layout.
pub fn memory_binding(binding: &LoweredMemoryBinding) -> Result<MemoryBinding> {
    validate_binding(binding)?;
    if binding.kind != MemoryKind::Agent
        || binding.namespace != format!("{}/agent:{}", binding.root, binding.agent_id)
    {
        return Err(Error::MemoryBindingUnsupported);
    }
    crate::HiveMemory::with_root(binding.root.clone())?.agent_id(&binding.agent_id)?;
    Ok(MemoryBinding::new(binding.agent_id.clone()).root(binding.root.clone()))
}

pub(crate) fn validate_binding(binding: &LoweredMemoryBinding) -> Result<()> {
    if binding.identity.kind != binding.kind
        || binding.identity.namespace != binding.namespace
        || binding.settings.identity != binding.identity.id
    {
        return Err(Error::MemoryBindingUnsupported);
    }
    Ok(())
}

/// Carry a lowered seat and its full memory contract to a host factory.
///
/// `config` retains the original nonsecret settings under `config`, alongside
/// the seat id, prompt, model, tools, MCP and skills. Factories consuming hive
/// definitions should validate and honor this portable envelope.
#[must_use]
pub fn management_request(seat: &LoweredSeat) -> ManagementRequest {
    ManagementRequest::CreateAgent {
        template: seat.template.clone(),
        config: serde_json::json!({
            "id": seat.id, "label": seat.label, "config": seat.config,
            "prompt": seat.prompt, "model": seat.model, "tools": seat.tools,
            "mcp": seat.mcp, "skills": seat.skills,
        }),
        memory: seat.memory.clone().map(Box::new),
    }
}

#[cfg(test)]
mod test;
