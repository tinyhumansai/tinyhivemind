//! Runtime boundary for one optional, tool-less responder selection call.

#[cfg(test)]
mod test;

use crate::Result;
use std::{error::Error as StdError, future::Future, pin::Pin};
use tinyhivemind_core::{desk::DeskSet, roster::Roster};

pub use tinyhivemind_core::responder::{
    ResponderDecision, ResponderPlan, ResponderRequest, ResponderRung, SelectionDisposition,
    SelectionPolicy, SelectionRequest, SelectorCandidate, accept_selection, responder_plan,
};

/// A boxed failure returned by a host selector implementation.
pub type BoxError = Box<dyn StdError + Send + Sync + 'static>;

/// The boxed, executor-neutral future returned by [`Selector`].
///
/// Its lifetime permits the future to borrow both the selector and its
/// [`SelectionRequest`]; neither borrow is required to be `'static`.
pub type SelectorFuture<'a> =
    Pin<Box<dyn Future<Output = std::result::Result<String, BoxError>> + Send + 'a>>;

/// A model-backed chooser with no transcript, tools, or host handles.
///
/// The trait is object-safe and receives only a raw message, canonical desk id,
/// and the bounded effective candidates assembled by the pure core.
pub trait Selector: Send + Sync {
    /// Return text intended to name one candidate id.
    ///
    /// The shared lifetime explicitly binds the returned future to both
    /// `self` and `request`, allowing an implementation to borrow either for
    /// the duration of this one selection call.
    fn select<'a>(&'a self, request: &'a SelectionRequest) -> SelectorFuture<'a>;
}

/// Choose exactly one responder, invoking the selector at most once.
///
/// Selector absence or failure uses the desk-default fallback with an
/// unavailable disposition. Invalid output uses the same fallback with an
/// invalid-output disposition. Only accepted output produces an auto-selection
/// decision.
///
/// # Errors
///
/// Returns typed pure-algebra failures from snapshot validation or a reached
/// fallback without an active responder. Selector failures are not errors.
pub async fn choose_responder(
    selector: Option<&(dyn Selector + '_)>,
    request: &ResponderRequest,
    roster: &Roster<'_>,
    desks: &DeskSet<'_>,
    candidate_details: &[SelectorCandidate],
) -> Result<ResponderDecision> {
    match responder_plan(request, roster, desks, candidate_details)? {
        ResponderPlan::Decided { decision } => Ok(decision),
        ResponderPlan::Select {
            request,
            mut fallback,
        } => {
            let Some(selector) = selector else {
                return Ok(fallback);
            };
            let Ok(output) = selector.select(&request).await else {
                return Ok(fallback);
            };
            let Some(responder_id) = accept_selection(&output, &request.candidates) else {
                fallback.disposition = SelectionDisposition::InvalidOutput;
                return Ok(fallback);
            };
            Ok(ResponderDecision {
                responder_id,
                rung: ResponderRung::AutoSelection,
                disposition: SelectionDisposition::Selected,
            })
        }
    }
}
