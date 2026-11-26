//! What a lowered seat may ask of an `OpenHuman` process, and what it gets.
//!
//! [`check`] reads a [`LoweredSeat`] against the `openhuman` column of the
//! capability table in `docs/specs/harnesses.md` and returns the [`SeatModel`]
//! the generated config is written from, or the first [`Refusal`]. Everything
//! the spec marks "host side" (hive and executable tools, hooks, permission
//! layers other than `sandbox`, `autonomy`, `access`, package MCP servers) is
//! the MCP server's business and is not judged here.
use tinyhivemind_lang::{GroupMode, LoweredSeat, ProviderKind, Sandbox};

/// Model iterations after which the released binary ends a custom agent's
/// turn, reporting it as a normal end.
pub const ITERATION_CAP: usize = 8;

/// The one provider route a seat runs on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeatModel {
    /// The provider's id in the package, for messages.
    pub provider: String,
    /// The Chat Completions base URL, as declared.
    pub endpoint: String,
    /// The provider's model name.
    pub model: String,
    /// Most model iterations a turn may start, at most [`ITERATION_CAP`].
    pub iterations: usize,
}

/// Why a seat cannot run on this harness. Each names the field.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum Refusal {
    /// The seat resolves to no model at all.
    #[error("the seat names no model")]
    NoModel,
    /// The model has no provider, so it would inherit one; a child process
    /// has none and never falls back to its own.
    #[error("model {model} names no provider, and a process harness has none to inherit")]
    NoProvider {
        /// The model's id.
        model: String,
    },
    /// The provider's kind is not one this harness has been shown to run.
    #[error("provider {provider} is of kind {kind}, which this harness does not run")]
    ProviderKind {
        /// The provider's id.
        provider: String,
        /// Its kind, as a package names it.
        kind: &'static str,
    },
    /// An `openai` provider without an explicit endpoint.
    #[error("provider {provider} has no endpoint")]
    NoEndpoint {
        /// The provider's id.
        provider: String,
    },
    /// A provider credential, which no conformance test has proven yet.
    #[error("provider {provider} has a credential, which this harness does not take yet")]
    Credential {
        /// The provider's id.
        provider: String,
    },
    /// A model setting the generated config cannot carry.
    #[error("model {model} sets {setting}, which this harness does not take")]
    ModelSetting {
        /// The model's id.
        model: String,
        /// `temperature` or `max_tokens`.
        setting: &'static str,
    },
    /// More iterations than the binary runs.
    #[error("the seat asks for {asked} iterations, more than the {cap} this harness runs")]
    Iterations {
        /// What the seat asked for.
        asked: usize,
        /// [`ITERATION_CAP`].
        cap: usize,
    },
    /// A spend limit, which nothing here can enforce.
    #[error("the seat sets limits.budget_usd, which this harness cannot enforce")]
    Budget,
    /// A sandbox, which would not reach the executable tools the host serves.
    #[error("permission set {permission} asks for a sandbox, which this harness cannot provide")]
    Sandbox {
        /// The permission set's id.
        permission: String,
    },
    /// Built-in tools, which start off for every process harness.
    #[error("the seat turns on built-in tool group {group}")]
    ToolGroup {
        /// The group.
        group: String,
    },
    /// A field this harness does not take at all yet.
    #[error("the seat sets {0}, which this harness does not take")]
    Unsupported(&'static str),
}

/// What `seat` runs on, if this harness can honour all of it.
///
/// # Errors
/// The first [`Refusal`], provider rows first.
pub fn check(seat: &LoweredSeat) -> Result<SeatModel, Refusal> {
    let integration = &seat.integration;
    let lowered = integration.model.as_ref().ok_or(Refusal::NoModel)?;
    let model = &lowered.model;
    let provider = lowered
        .provider
        .as_ref()
        .ok_or_else(|| Refusal::NoProvider {
            model: model.id.clone(),
        })?;
    let kind = match provider.kind {
        ProviderKind::Openai => None,
        ProviderKind::Inherit => Some("inherit"),
        ProviderKind::Openrouter => Some("openrouter"),
        ProviderKind::Anthropic => Some("anthropic"),
    };
    if let Some(kind) = kind {
        return Err(Refusal::ProviderKind {
            provider: provider.id.clone(),
            kind,
        });
    }
    let endpoint = provider
        .endpoint
        .clone()
        .filter(|endpoint| !endpoint.trim().is_empty())
        .ok_or_else(|| Refusal::NoEndpoint {
            provider: provider.id.clone(),
        })?;
    if provider.credential.is_some() {
        return Err(Refusal::Credential {
            provider: provider.id.clone(),
        });
    }
    for (set, setting) in [
        (model.temperature.is_some(), "temperature"),
        (model.max_tokens.is_some(), "max_tokens"),
    ] {
        if set {
            return Err(Refusal::ModelSetting {
                model: model.id.clone(),
                setting,
            });
        }
    }
    let iterations = integration.limits.iterations.unwrap_or(ITERATION_CAP);
    if iterations > ITERATION_CAP {
        return Err(Refusal::Iterations {
            asked: iterations,
            cap: ITERATION_CAP,
        });
    }
    if integration.limits.budget_usd.is_some() {
        return Err(Refusal::Budget);
    }
    unsupported(seat)?;
    Ok(SeatModel {
        provider: provider.id.clone(),
        endpoint,
        model: model.name.clone(),
        iterations,
    })
}

/// Everything else the table rejects for this kind.
fn unsupported(seat: &LoweredSeat) -> Result<(), Refusal> {
    let integration = &seat.integration;
    if let Some(set) = integration
        .permissions
        .iter()
        .find(|set| set.sandbox != Sandbox::None)
    {
        return Err(Refusal::Sandbox {
            permission: set.id.clone(),
        });
    }
    if let Some((group, _)) = integration
        .groups
        .iter()
        .find(|(_, mode)| **mode != GroupMode::Off)
    {
        return Err(Refusal::ToolGroup {
            group: group.clone(),
        });
    }
    let fields = [
        (seat.memory.is_some(), "memory"),
        (integration.agent_id.is_some(), "agent_id"),
        (integration.definition_base.is_some(), "definition_base"),
        (!integration.subagents.is_empty(), "subagents"),
        (!integration.skills.is_empty(), "skills"),
        (!integration.context.is_empty(), "context"),
        (integration.bare_prompt, "bare_prompt"),
    ];
    match fields.into_iter().find(|(set, _)| *set) {
        Some((_, field)) => Err(Refusal::Unsupported(field)),
        None => Ok(()),
    }
}

#[cfg(test)]
mod test;
