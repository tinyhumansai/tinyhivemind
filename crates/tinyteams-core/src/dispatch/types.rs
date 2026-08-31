//! Stable payloads for one bounded mention-dispatch decision.

use crate::mention::Mention;
use serde::{Deserialize, Deserializer, Serialize};

/// Explicit host policy for agent-to-agent mention dispatch.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct MentionDispatchPolicy {
    /// Whether mention dispatch is enabled for this committed reply.
    pub enabled: bool,
    /// Maximum permitted chain depth, supplied by the host without a library cap.
    pub max_hops: u32,
}

/// The committed trigger that makes an enqueue idempotent within its scope.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DispatchKey {
    /// Host-owned sequence of the committed agent reply.
    pub trigger_sequence: u64,
}

/// Pure conversation identity bound into a mention turn request.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DispatchConversation {
    /// Canonical case-sensitive desk id.
    pub desk_id: String,
    /// Root sequence for a thread, or explicit `null` for the desk channel.
    #[serde(deserialize_with = "deserialize_required_option")]
    pub thread_root: Option<u64>,
}

/// Inputs captured from one committed agent reply.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct MentionDispatchInput {
    /// Idempotency key for the committed reply.
    pub key: DispatchKey,
    /// Conversation to which any child turn remains bound.
    pub conversation: DispatchConversation,
    /// Exact active agent id that authored the reply.
    pub author_id: String,
    /// Exact committed reply content passed to the child turn.
    pub content: String,
    /// Revalidated resolved mentions from the committed content.
    pub mentions: Vec<Mention>,
    /// Current chain depth of the committed reply.
    pub hop: u32,
}

/// Why no child turn was selected.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoDispatchReason {
    /// Host policy explicitly disabled dispatch.
    Disabled,
    /// The current reply has exhausted the host's hop budget.
    HopLimitReached,
    /// The committed author is not an active agent.
    SourceInactive,
    /// No nonquiet direct agent mention was present.
    NoDirectAgentMention,
    /// The first direct mention addresses the author.
    SelfMention,
    /// The first direct mention addresses an inactive agent.
    TargetInactive,
    /// The child hop could not be represented.
    HopOverflow,
}

/// One canonical child-turn enqueue request.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct MentionTurnRequest {
    /// Idempotency key derived from the committed reply.
    pub key: DispatchKey,
    /// Exact active author id.
    pub source_id: String,
    /// Exact active target id.
    pub target_id: String,
    /// Exact committed reply content.
    pub content: String,
    /// Bound conversation scope.
    pub conversation: DispatchConversation,
    /// Checked child depth.
    pub child_hop: u32,
}

/// Pure result of considering one committed reply.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MentionDispatchDecision {
    /// No enqueue may be attempted.
    None {
        /// Deterministic reason for stopping.
        reason: NoDispatchReason,
    },
    /// Exactly one canonical enqueue may be attempted.
    One {
        /// Request to pass unchanged to the host queue.
        request: MentionTurnRequest,
    },
}

fn deserialize_required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}
