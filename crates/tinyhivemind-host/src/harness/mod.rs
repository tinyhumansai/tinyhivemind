//! The port a harness client implements, and its neutral payloads.
//!
//! A [`Harness`] starts a seat and returns the [`AgentRunner`] the coordinator
//! drives it through. Nothing here names a client's types, and nothing here
//! talks to a harness; a client crate does both.
//!
//! What every client shares is here so that it is written once:
//!
//! - [`SeatRunner`] holds a seat to one turn at a time, one session for life
//!   and no turn twice, and settles a turn that was dropped, failed or timed
//!   out before the next one starts. A client implements
//!   [`CancellableRunner`] and returns its runner wrapped.
//! - [`Expected::verify`] refuses an instance that is not the child that was
//!   started, not the pinned version, or offers any other set of tools.
//! - [`TurnUsage::since`] turns a harness's running total into a turn's usage.
//!
//! The `conformance` feature adds the suite every client must pass.
mod error;
mod events;
mod kind;
mod plan;
mod seat_runner;
mod verification;

pub use error::{Error, Result};
pub use events::{
    ApprovalFuture, Approver, DenyAll, Event, Ignore, Observer, Progress, ToolApproval,
    ToolDecision, TurnUsage,
};
pub use kind::HarnessKind;
pub use plan::{SeatPlan, ToolEndpoint};
pub use seat_runner::{CancelFuture, CancellableRunner, SeatRunner};
pub use verification::{Expected, Mismatch, Started};

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tinyhivemind_hives::AgentRunner;

/// Future returned by [`Harness::start`].
pub type StartFuture = Pin<Box<dyn Future<Output = Result<Arc<dyn AgentRunner>>> + Send>>;
/// Future returned by [`Harness::stop`].
pub type StopFuture = Pin<Box<dyn Future<Output = Result<()>> + Send>>;

/// One kind of harness, able to run seats.
///
/// Dropping a harness and every runner it returned also ends its processes,
/// but without proof; [`Self::stop`] is how a host learns that they are gone.
pub trait Harness: Send + Sync {
    /// Which harness this is.
    fn kind(&self) -> HarnessKind;

    /// Start `seat`, offering it `tools` and nothing else, and verify what was
    /// started. The future fails, leaving nothing running, unless the instance
    /// is exactly what was asked for; on success it has reported
    /// [`Event::Started`] to the plan's observer.
    fn start(&self, seat: SeatPlan, tools: ToolEndpoint) -> StartFuture;

    /// Stop the seat with this id, along with its instance if no other seat
    /// shares it, and prove that no process of it is left. A seat that is not
    /// running is already stopped. The seat's state stays, so it can be
    /// started again and its session resumed.
    fn stop(&self, seat_id: &str) -> StopFuture;
}

#[cfg(test)]
mod test;
