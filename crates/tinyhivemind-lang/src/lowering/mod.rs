//! Translate validated packages into existing core values and host requests.
mod types;
use crate::{Package, Result};
use tinyhivemind_core::embed::RouteCandidate;
pub use types::*;
/// Lower a validated package; the host chooses when to activate the snapshot.
/// # Errors
/// Returns validation errors before producing any runtime configuration.
pub fn lower(package: &Package) -> Result<LoweredHive> {
    crate::validate(package)?;
    let seats = package
        .manifest
        .seats
        .iter()
        .map(|seat| {
            let prompt = body(&package.documents[&seat.prompt]).to_owned();
            let memory = package
                .memory
                .bindings
                .iter()
                .find(|binding| binding.seat == seat.id)
                .and_then(|binding| {
                    package
                        .memory
                        .identities
                        .iter()
                        .find(|identity| identity.id == binding.identity)
                        .map(|identity| LoweredMemoryBinding {
                            agent_id: identity
                                .namespace
                                .rsplit_once(':')
                                .map_or_else(|| identity.namespace.clone(), |(_, id)| id.into()),
                            root: identity.namespace.rsplit_once('/').map_or_else(
                                || package.memory.root.clone(),
                                |(root, _)| root.into(),
                            ),
                            namespace: identity.namespace.clone(),
                            kind: identity.kind,
                            identity: identity.clone(),
                            settings: binding.clone(),
                        })
                });
            LoweredSeat {
                id: seat.id.clone(),
                label: seat.label.clone(),
                template: seat.template.clone(),
                config: seat.config.clone(),
                prompt,
                model: seat.model.clone(),
                tools: seat.tools.clone(),
                mcp: seat.mcp.clone(),
                skills: seat.skills.clone(),
                memory,
            }
        })
        .collect();
    let candidates = package
        .manifest
        .seats
        .iter()
        .map(|seat| RouteCandidate {
            id: seat.id.clone(),
            label: seat.label.clone(),
            role: Some(seat.role.clone()),
            description: package
                .manifest
                .roles
                .iter()
                .find(|role| role.id == seat.role)
                .map(|role| body(&package.documents[&role.charter]).to_owned()),
            capabilities: seat.tools.allow.clone(),
            learned_topics: vec![],
            available: true,
        })
        .collect();
    Ok(LoweredHive {
        id: package.manifest.id.clone(),
        name: package.manifest.name.clone(),
        goals: package.manifest.goals.clone(),
        missions: package.manifest.missions.clone(),
        topology: package.manifest.topology.clone(),
        referrals: package.manifest.referrals.clone(),
        context: package
            .manifest
            .context
            .iter()
            .map(|path| (path.clone(), package.documents[path].clone()))
            .collect(),
        desks: package.manifest.desks.clone(),
        candidates,
        seats,
        policies: package.manifest.policies.clone(),
        coordinator: package.manifest.coordinator.clone(),
        memory: package.memory.clone(),
    })
}

fn body(source: &str) -> &str {
    source
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---\n"))
        .map_or(source, |(_, body)| body)
}

#[cfg(test)]
mod test;
