//! Explicit conversions of portable definitions at the `OpenHuman` boundary.
//!
//! [`PackageSource`], the plan it lowers into and the neutral conversions are
//! `tinyhivemind-host`'s, re-exported here; this module adds the `OpenHuman`
//! memory binding.
use crate::{Error, Result};
use openhuman_embed::MemoryBinding;
pub use tinyhivemind_host::language::{PackageSource, coordinator_options, management_request};
pub(crate) use tinyhivemind_host::language::plan;
use tinyhivemind_lang::{LoweredMemoryBinding, MemoryKind};

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

#[cfg(test)]
mod test;
