//! The ports through which a host decides what an agent may do.
//!
//! The belt in [`crate::tools`] consults them; it decides nothing itself.
mod types;
use crate::Result;
use std::{future::Future, pin::Pin};
pub use types::{ManagementRequest, NewAgent, SendRequest};

/// Explicit opt-in management authorization.
pub trait ManagementAuthorizer: Send + Sync {
    /// Authorize before any factory or coordinator mutation.
    /// # Errors
    /// Return a denial when the actor cannot perform this request.
    fn authorize(&self, actor: &str, request: &ManagementRequest) -> Result<()>;
}
/// Host policy over what an agent may send, mirroring [`ManagementAuthorizer`].
///
/// Consulted by `hivemind_send_agent`, `hivemind_send_hive`, `hivemind_ask` and
/// `hivemind_broadcast` before they execute. A refusal is returned to the model
/// as the tool's error text; the turn continues.
pub trait SendAuthorizer: Send + Sync {
    /// Admit or refuse one outbound request from `actor`.
    /// # Errors
    /// Return a refusal, conventionally [`crate::Error::SendDenied`], whose
    /// message the model reads.
    fn authorize(&self, actor: &str, request: &SendRequest) -> Result<()>;
    /// Authorize a send from the native session bound by the tool source.
    /// # Errors
    /// Returns the same refusal as `authorize` unless the host narrows sessions.
    fn authorize_in_session(
        &self,
        actor: &str,
        request: &SendRequest,
        _session: Option<&str>,
    ) -> Result<()> {
        self.authorize(actor, request)
    }
}
/// Resolves to the coordinator id of a created and registered agent.
pub type CreateFuture<'a> = Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>>;
/// Builds an agent on the host's harness and registers it with the coordinator.
///
/// The one management step that needs a harness; everything else a management
/// request does is a coordinator call. Called only after authorization.
pub trait AgentCreator: Send + Sync {
    /// Build and register `agent`, resolving to its coordinator id.
    fn create(&self, agent: NewAgent) -> CreateFuture<'_>;
}
/// Management as a host configured it: who may do what, and who builds agents.
#[derive(Clone, Copy)]
pub struct Management<'a> {
    /// Consulted before any mutation.
    pub authorizer: &'a dyn ManagementAuthorizer,
    /// Builds the agents a request creates.
    pub creator: &'a dyn AgentCreator,
}
impl std::fmt::Debug for Management<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Management")
    }
}

#[cfg(test)]
mod test;
