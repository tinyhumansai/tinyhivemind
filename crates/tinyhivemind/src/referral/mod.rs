//! One-call runtime edge from a pure cross-desk referral decision to a
//! host-owned atomic queue.
//!
//! This mirrors [`crate::dispatch`] exactly, and differs in one thing: the
//! child turn it enqueues may run on a *different* conversation from the one
//! that triggered it. The host therefore has two conversations to authorize
//! rather than one, and the idempotency key is scoped by the referral's
//! `from` conversation and trigger sequence.

#[cfg(test)]
mod test;

mod types;

pub use types::ReferralOutcome;

use crate::{BoxError, EnqueueOutcome, Result};
use std::{future::Future, pin::Pin};
pub use tinyhivemind_core::referral::{
    NoReferralReason, Referral, ReferralDecision, ReferralInput, ReferralKind, ReferralOrigin,
    ReferralPolicy, ReferralReach, referral,
};
use tinyhivemind_core::{desk::DeskSet, roster::Roster};

/// Boxed executor-neutral future returned by [`ReferralQueue`].
pub type ReferralFuture<'a> =
    Pin<Box<dyn Future<Output = std::result::Result<EnqueueOutcome, BoxError>> + Send + 'a>>;

/// Host-owned atomic enqueue boundary for one referred child turn.
///
/// An implementation must perform one atomic transaction keyed by
/// `referral.from` plus `referral.key.trigger_sequence`. Within it, the host
/// must re-read the stored committed agent reply and verify its source,
/// content, conversation and sequence; revalidate the live feature policy,
/// authorization and target availability **on both conversations**, since a
/// crossing referral writes into a channel the author is not a member of; and
/// durably enqueue at most one child turn. A duplicate returns
/// [`EnqueueOutcome::Already`], and an expected live rejection returns
/// [`EnqueueOutcome::Refused`]. Transaction rollback must leave neither an
/// idempotency record nor a child turn.
///
/// This port is the only idempotency boundary. `tinyhivemind` owns no journal
/// and does not retry a failure or refusal.
pub trait ReferralQueue: Send + Sync {
    /// Atomically enqueue this one canonical referral, or return its final
    /// refusal/duplicate outcome.
    fn enqueue_once(&self, referral: Referral) -> ReferralFuture<'_>;
}

/// Decide and attempt at most one referred child-turn enqueue.
///
/// A pure no-referral result calls the queue zero times. A one-target result
/// calls it exactly once and maps all expected refusals without retrying or
/// considering a later mention. `policy` carries the host's explicit
/// enablement and finite configurable hop limit; the library adds no smaller
/// hard ceiling.
///
/// # Errors
///
/// Returns a typed core snapshot error, or [`crate::Error::Enqueue`]
/// preserving an unexpected host queue failure as its source.
pub async fn dispatch_referral(
    queue: &(dyn ReferralQueue + '_),
    policy: ReferralPolicy,
    input: &ReferralInput,
    roster: &Roster<'_>,
    desks: &DeskSet<'_>,
) -> Result<ReferralOutcome> {
    let one = match referral(policy, input, roster, desks)? {
        ReferralDecision::None { reason } => return Ok(ReferralOutcome::NotReferred { reason }),
        ReferralDecision::One { referral } => *referral,
    };
    let crossed = one.crosses();
    let outcome = queue
        .enqueue_once(one)
        .await
        .map_err(|source| crate::Error::Enqueue { source })?;
    Ok(match outcome {
        EnqueueOutcome::Enqueued => ReferralOutcome::Referred { crossed },
        EnqueueOutcome::Already => ReferralOutcome::Already,
        EnqueueOutcome::Refused { reason } => ReferralOutcome::Refused { reason },
    })
}
