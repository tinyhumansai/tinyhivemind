//! Runtime outcomes for the atomic referral enqueue boundary.

use crate::EnqueueRefusal;
use serde::{Deserialize, Serialize};
use tinyhivemind_core::referral::NoReferralReason;

/// Final result of one library referral attempt.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ReferralOutcome {
    /// Pure policy or routing selected no target, so the queue was not called.
    NotReferred {
        /// Deterministic reason for stopping.
        reason: NoReferralReason,
    },
    /// The host durably created the child turn.
    Referred {
        /// Whether that turn runs on a different conversation from the trigger.
        crossed: bool,
    },
    /// The exact scoped trigger had already created its child turn.
    Already,
    /// The host atomically refused the request after revalidation.
    Refused {
        /// Expected refusal returned by the host.
        reason: EnqueueRefusal,
    },
}
