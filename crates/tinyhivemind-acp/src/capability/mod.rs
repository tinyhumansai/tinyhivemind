//! The one model route a seat may take on an ACP agent.
//!
//! [`check`] reads a [`LoweredSeat`] for what this runner needs to start it:
//! a model whose provider is declared with an explicit endpoint and no
//! credential, of the provider kind the agent speaks ([`HarnessKind::model_api`]).
//! The rest of the capability table is the package checker's: it already
//! rejected, at the field, whatever no agent here can enforce.
use tinyhivemind_lang::{HarnessKind, LoweredSeat, ModelApi, ProviderKind};

/// The model route of a seat.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeatModel {
    /// The provider's id in the package, for messages.
    pub provider: String,
    /// Its endpoint, as declared.
    pub endpoint: String,
    /// The provider's model name.
    pub model: String,
}

/// Why a seat cannot run on this agent. Each names the field.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum Refusal {
    /// The seat resolves to no model.
    #[error("seat {0} has no model")]
    NoModel(String),
    /// The seat's model has no provider: an agent process has none of its
    /// own to fall back on.
    #[error("model {0} has no provider")]
    NoProvider(String),
    /// The provider is not of the kind the agent speaks.
    #[error("provider {provider} is {found:?}; a {kind} seat needs {needed:?}")]
    ProviderKind {
        /// The provider.
        provider: String,
        /// Its kind.
        found: ProviderKind,
        /// The kind the agent speaks.
        needed: ProviderKind,
        /// The seat's harness kind.
        kind: HarnessKind,
    },
    /// The provider declares no endpoint, so the agent would reach its
    /// vendor's own, which needs a key.
    #[error("provider {0} has no endpoint")]
    NoEndpoint(String),
    /// The provider declares a credential, which never reaches an agent.
    #[error("provider {0} declares a credential")]
    Credential(String),
}

/// The provider kind an agent of `kind` speaks to.
#[must_use]
pub const fn provider_kind(kind: HarnessKind) -> ProviderKind {
    match kind.model_api() {
        ModelApi::Messages => ProviderKind::Anthropic,
        ModelApi::ChatCompletions | ModelApi::Responses => ProviderKind::Openai,
    }
}

/// The route `seat` takes on an agent of `kind`, or why it cannot run there.
///
/// # Errors
/// The first [`Refusal`].
pub fn check(seat: &LoweredSeat, kind: HarnessKind) -> Result<SeatModel, Refusal> {
    let lowered = seat
        .integration
        .model
        .as_ref()
        .ok_or_else(|| Refusal::NoModel(seat.id.clone()))?;
    let provider = lowered
        .provider
        .as_ref()
        .ok_or_else(|| Refusal::NoProvider(lowered.model.id.clone()))?;
    let needed = provider_kind(kind);
    if provider.kind != needed {
        return Err(Refusal::ProviderKind {
            provider: provider.id.clone(),
            found: provider.kind,
            needed,
            kind,
        });
    }
    if provider.credential.is_some() {
        return Err(Refusal::Credential(provider.id.clone()));
    }
    let endpoint = provider
        .endpoint
        .clone()
        .ok_or_else(|| Refusal::NoEndpoint(provider.id.clone()))?;
    Ok(SeatModel {
        provider: provider.id.clone(),
        endpoint,
        model: lowered.model.name.clone(),
    })
}

#[cfg(test)]
mod test;
