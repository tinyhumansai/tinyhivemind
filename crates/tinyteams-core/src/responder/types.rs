//! Stable responder selection inputs and decisions.

use serde::{Deserialize, Serialize};

use crate::mention::Mention;

/// Descriptive selector input for one active desk member.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SelectorCandidate {
    /// Canonical agent id.
    pub id: String,
    /// Human-readable agent label.
    pub label: String,
    /// Team role supplied to the selector.
    pub role: String,
    /// Optional short capability description.
    pub description: Option<String>,
}

/// Whether a model-assisted selection rung may run.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionPolicy {
    /// The runtime may invoke its selector port.
    Allowed,
    /// Selection is disabled and the deterministic fallback wins.
    Disabled,
}

/// Caller-owned inputs to the pure responder ladder.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponderRequest {
    /// The raw authored message.
    pub message: String,
    /// Stored chat identity, or no identity for General.
    pub chat: Option<String>,
    /// Already-resolved mentions for the message.
    pub mentions: Vec<Mention>,
    /// Canonical id of the host's orchestrator agent.
    pub orchestrator_id: String,
    /// Whether model-assisted selection is enabled for this request.
    pub selection_policy: SelectionPolicy,
}

/// The complete, bounded input visible to a model selector.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SelectionRequest {
    /// The raw authored message.
    pub message: String,
    /// Canonical desk id.
    pub desk_id: String,
    /// Effective active candidates in desk order.
    pub candidates: Vec<SelectorCandidate>,
}

/// The ladder rung that produced a responder.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponderRung {
    /// A direct, active agent mention.
    ExplicitMention,
    /// Model-assisted selection or its deterministic fallback.
    AutoSelection,
    /// The first effective active member of a desk.
    DeskDefault,
    /// A direct-message or bare agent chat identity.
    DirectAgent,
    /// The host's orchestrator fallback.
    Orchestrator,
}

/// What happened at the optional selector boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionDisposition {
    /// No model selection applied to this rung.
    NotApplicable,
    /// A selector returned a valid candidate.
    Selected,
    /// Selection policy disabled the model rung.
    Disabled,
    /// No selector was available or it failed.
    Unavailable,
    /// Selector output did not name exactly one candidate.
    InvalidOutput,
}

/// The single responder selected for one input message.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ResponderDecision {
    /// Canonical active agent id.
    pub responder_id: String,
    /// Ladder rung that selected the id.
    pub rung: ResponderRung,
    /// Selector outcome, when the auto rung applied.
    pub disposition: SelectionDisposition,
}

/// A pure plan that either decides immediately or requests one selector call.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResponderPlan {
    /// No runtime selection is needed.
    Decided {
        /// The sole responder decision.
        decision: ResponderDecision,
    },
    /// Invoke a selector once, falling back deterministically.
    Select {
        /// The bounded request visible to the selector.
        request: SelectionRequest,
        /// First-candidate fallback for unavailable or invalid selection.
        fallback: ResponderDecision,
    },
}
