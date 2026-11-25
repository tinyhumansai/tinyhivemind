//! Host extension points and supplied handle types.
use crate::{Error, Result};
use openhuman_embed::Agent;
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};
use tinyhivemind_hives::TurnDisposition;
pub use tinyhivemind_host::{
    policy::{ManagementRequest, SendRequest},
    turn::{TurnOptions, TurnScope},
};
use tinyhivemind_lang::LoweredMemoryBinding;
/// Default maximum duration of a supplied agent turn.
pub const TURN_TIMEOUT: Duration = Duration::from_mins(5);
/// Host factory result future.
pub type AgentFuture = Pin<Box<dyn Future<Output = Result<Agent>> + Send>>;
/// Host creates configured agents from nonsecret template references.
///
/// The factory owns the runtime and provider credentials. It must return a
/// configured handle from the host's runtime; the adapter rejects other runtimes.
pub trait AgentFactory: Send + Sync {
    /// Create on the shared runtime; host validates template and config.
    fn create(&self, template: String, config: serde_json::Value) -> AgentFuture;
    /// Create with an explicit memory contract before building the agent.
    ///
    /// Override to apply the binding to `AgentSpec` and implement every supplied
    /// memory setting, including read-only identities, recall, reaches and
    /// lifecycle. The default refuses bindings and preserves old factories for
    /// unbound requests. The adapter verifies the resulting agent id and root.
    fn create_with_memory(
        &self,
        template: String,
        config: serde_json::Value,
        memory: Option<LoweredMemoryBinding>,
    ) -> AgentFuture {
        if memory.is_some() {
            Box::pin(async { Err(Error::MemoryBindingUnsupported) })
        } else {
            self.create(template, config)
        }
    }
}
/// Explicit opt-in management authorization.
///
/// [`tinyhivemind_host::policy::ManagementAuthorizer`] with the adapter's error.
pub trait ManagementAuthorizer: Send + Sync {
    /// Authorize before any factory or coordinator mutation.
    /// # Errors
    /// Return a denial when the actor cannot perform this request.
    fn authorize(&self, actor: &str, request: &ManagementRequest) -> Result<()>;
}
/// Host policy over what an agent may send, mirroring [`ManagementAuthorizer`].
///
/// [`tinyhivemind_host::policy::SendAuthorizer`] with the adapter's error.
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
/// Future wrapped by a host turn scope.
pub type HostedTurn<'a> =
    Pin<Box<dyn Future<Output = Result<openhuman_embed::TurnOutcome>> + Send + 'a>>;
/// Drained progress channel owned by the host.
pub type TurnProgressSink =
    tokio::sync::mpsc::Sender<openhuman_embed::agent_progress::AgentProgress>;
/// Optional per-turn options, progress, usage, approval and scope hooks.
///
/// Called in order: `turn_timeout`, `context`, `prepare`, `configure`,
/// `progress`, `wrap_turn`, then `after_turn`.
pub trait TurnHooks: Send + Sync {
    /// Override the host's fallback native deadline for this captured turn.
    /// `None` retains the fallback; a zero duration refuses dispatch.
    /// This deadline is applied after `configure` so timeout classification and
    /// native cancellation use the same bound.
    fn turn_timeout(&self, _scope: &TurnScope) -> Option<std::time::Duration> {
        None
    }
    /// Add context bound to this authoritative coordinator request.
    fn context(&self, _scope: &TurnScope) -> String {
        String::new()
    }
    /// Add asynchronously recalled context before the native turn is built.
    fn context_async<'a>(
        &'a self,
        scope: &'a TurnScope,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(async move { self.context(scope) })
    }
    /// Configure a native turn, adding permission hooks or model budgets.
    /// Use `turn_timeout` for deadline overrides; its bound is applied afterward.
    fn configure(&self, _scope: &TurnScope, turn: openhuman_embed::Turn) -> openhuman_embed::Turn {
        turn
    }
    /// Choose this turn's options before it is built.
    fn prepare(&self, _scope: &TurnScope) -> TurnOptions {
        TurnOptions::default()
    }
    /// Host owes this sink a receiver throughout the turn.
    fn progress(&self, _scope: &TurnScope) -> Option<TurnProgressSink> {
        None
    }
    /// Install host context around the complete turn.
    fn wrap_turn<'a>(&'a self, _scope: &'a TurnScope, turn: HostedTurn<'a>) -> HostedTurn<'a> {
        turn
    }
    /// Called after successful or failed turns with fresh usage, never stale usage.
    /// # Errors
    /// Host metering or approval processing can refuse completion. When the
    /// agent already committed its turn, its session remains bound while the
    /// coordinator interrupts delivery and discards staged actions and replies.
    fn after_turn(
        &self,
        _scope: &TurnScope,
        _usage: Option<&openhuman_core::agent::tinyagents::host::LastTurnUsage>,
    ) -> Result<TurnDisposition> {
        Ok(TurnDisposition::Completed)
    }
}
#[derive(Debug)]
pub(super) struct DefaultHooks;
impl TurnHooks for DefaultHooks {}
/// An existing agent implementing core's bound-handle trait.
///
/// For hosts driving the pure completion driver directly. The trait's historical
/// `runtime_id` method returns this agent's ID; it is distinct from
/// [`Agent::runtime_id`], which identifies the shared `OpenHuman` runtime.
#[derive(Clone, Debug)]
pub struct RegisteredAgent(pub Agent);
impl tinyhivemind_core::driver::BoundAgent for RegisteredAgent {
    fn runtime_id(&self) -> &str {
        self.0.id()
    }
}
pub(super) struct Management {
    pub factory: Arc<dyn AgentFactory>,
    pub authorizer: Arc<dyn ManagementAuthorizer>,
}
