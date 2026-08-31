//! One-call runtime edge from a pure mention decision to a host-owned queue.

#[cfg(test)]
mod test;

mod types;

pub use types::{EnqueueOutcome, EnqueueRefusal, MentionDispatchOutcome};

use crate::{BoxError, Result};
use std::{future::Future, pin::Pin};
pub use tinyteams_core::dispatch::{
    DispatchConversation, DispatchKey, MentionDispatchDecision, MentionDispatchInput,
    MentionDispatchPolicy, MentionTurnRequest, NoDispatchReason, mention_dispatch,
};
use tinyteams_core::roster::Roster;

/// Boxed executor-neutral future returned by [`MentionTurnQueue`].
pub type MentionTurnFuture<'a> =
    Pin<Box<dyn Future<Output = std::result::Result<EnqueueOutcome, BoxError>> + Send + 'a>>;

/// Host-owned atomic enqueue boundary for one mentioned child turn.
///
/// An implementation must perform one atomic transaction keyed by the bound
/// conversation plus `request.key.trigger_sequence`. Within it, the host must
/// re-read the stored committed agent reply and verify its source, content,
/// conversation and sequence; revalidate the live feature policy,
/// authorization and target availability; and durably enqueue at most one
/// child turn. A duplicate returns [`EnqueueOutcome::Already`], and an expected
/// live rejection returns [`EnqueueOutcome::Refused`]. Transaction rollback
/// must leave neither an idempotency record nor a child turn.
///
/// This port is the only idempotency boundary. `tinyteams` owns no journal and
/// does not retry a failure or refusal.
pub trait MentionTurnQueue: Send + Sync {
    /// Atomically enqueue this one canonical request, or return its final
    /// refusal/duplicate outcome.
    fn enqueue_once(&self, request: MentionTurnRequest) -> MentionTurnFuture<'_>;
}

/// Decide and attempt at most one mentioned child-turn enqueue.
///
/// A pure no-dispatch result calls the queue zero times. A one-target result
/// calls it exactly once and maps all expected refusals without retrying or
/// considering a later mention.
///
/// # Errors
///
/// Returns a typed core snapshot error, or [`crate::Error::Enqueue`] preserving
/// an unexpected host queue failure as its source.
pub async fn dispatch_mention(
    queue: &(dyn MentionTurnQueue + '_),
    policy: MentionDispatchPolicy,
    input: &MentionDispatchInput,
    roster: &Roster<'_>,
) -> Result<MentionDispatchOutcome> {
    let request = match mention_dispatch(policy, input, roster)? {
        MentionDispatchDecision::None { reason } => {
            return Ok(MentionDispatchOutcome::NotDispatched { reason });
        }
        MentionDispatchDecision::One { request } => request,
    };
    let outcome = queue
        .enqueue_once(request)
        .await
        .map_err(|source| crate::Error::Enqueue { source })?;
    Ok(match outcome {
        EnqueueOutcome::Enqueued => MentionDispatchOutcome::Enqueued,
        EnqueueOutcome::Already => MentionDispatchOutcome::Already,
        EnqueueOutcome::Refused { reason } => MentionDispatchOutcome::Refused { reason },
    })
}
