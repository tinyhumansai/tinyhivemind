//! Runtime outcomes for the atomic mention-turn enqueue boundary.

use serde::{Deserialize, Serialize};
use tinyteams_core::dispatch::NoDispatchReason;

/// Result returned by a host's atomic enqueue transaction.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EnqueueOutcome {
    /// This request durably created its one child turn.
    Enqueued,
    /// The bound conversation-and-trigger key was already enqueued.
    Already,
    /// Current authorization no longer permits this source-target edge.
    Unauthorized,
    /// The target is no longer available for a turn.
    TargetUnavailable,
    /// Current host policy no longer enables the feature.
    FeatureDisabled,
}

/// Final result of one library dispatch attempt.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum MentionDispatchOutcome {
    /// Pure policy or routing selected no target, so the queue was not called.
    NotDispatched {
        /// Deterministic reason for stopping.
        reason: NoDispatchReason,
    },
    /// The host durably created the child turn.
    Enqueued,
    /// The exact scoped trigger had already created its child turn.
    Already,
    /// The host atomically refused the request after revalidation.
    Refused {
        /// Expected refusal returned by the host.
        outcome: EnqueueOutcome,
    },
}
