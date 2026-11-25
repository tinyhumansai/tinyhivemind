//! What the suite needs of a client under test.
use crate::harness::{Harness, SeatPlan, ToolEndpoint, TurnUsage};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

/// Future returned by [`Subject::escapee`].
pub type EscapeFuture = Pin<Box<dyn Future<Output = Option<u32>> + Send>>;

/// What the scripted model does with its next call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelStep {
    /// Answer.
    Reply {
        /// The reply.
        text: String,
        /// What the call reports it used. The suite sets input tokens only.
        usage: TurnUsage,
    },
    /// Never answer.
    Hang,
}

/// How a model call stands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CallState {
    /// Waiting for the model.
    Open,
    /// Answered.
    Answered,
    /// Closed by the harness, or by its death, before the model answered.
    Aborted,
}

/// One call the scripted model received.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelCall {
    /// User messages in it. A later turn of a session sees the earlier ones.
    pub seen: usize,
    /// How it stands.
    pub state: CallState,
}

/// What the instance does with its tool list.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ToolFault {
    /// Offers what it was given.
    None,
    /// Offers another tool as well.
    Extra,
    /// Leaves one out.
    Missing,
}

/// A harness client with a scripted model and fresh durable state.
///
/// What it reports has to be observed from outside the client: the model calls
/// at the model, the processes in the system's own table.
pub trait Subject: Send + Sync {
    /// A new harness over this subject's durable state, as after a restart of
    /// the host.
    fn harness(&self) -> Arc<dyn Harness>;
    /// The seat to start, in the same directory every time. The suite sets the
    /// observer and the turn limit.
    fn seat(&self) -> SeatPlan;
    /// The seat's tools. Nothing needs to be served there.
    fn endpoint(&self) -> ToolEndpoint;
    /// Append a step to the model's script.
    fn script(&self, step: ModelStep);
    /// Every call the model received, in order.
    fn model_calls(&self) -> Vec<ModelCall>;
    /// Delete the seat's sessions from the durable state of a stopped seat.
    fn lose_sessions(&self);
    /// Make every instance started from now on offer its tools this way.
    fn offer(&self, fault: ToolFault);
    /// Have the running instance start a process in a session of its own, and
    /// return its pid, or `None` if this kind cannot be made to.
    fn escapee(&self) -> EscapeFuture;
    /// Every running process of the subject's instances.
    fn processes(&self) -> Vec<u32>;
    /// Longest the suite waits for anything. Generous, such as a minute, since
    /// only a failure takes it.
    fn patience(&self) -> Duration;
}
