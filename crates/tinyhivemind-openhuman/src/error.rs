//! Typed adapter errors.
/// Adapter result.
pub type Result<T> = std::result::Result<T, Error>;
/// Failures attaching or running supplied agents.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The agent or coordinator belongs to another runtime.
    #[error("agent or coordinator belongs to another runtime")]
    RuntimeMismatch,
    /// Another live handle has this agent ID.
    #[error("conflicting agent handle {0}")]
    AgentConflict(String),
    /// Management settings cannot change after registration.
    #[error("management must be configured before registration")]
    ManagementAlreadyStarted,
    /// Management was not configured by the host.
    #[error("management is disabled")]
    ManagementDisabled,
    /// The factory has not opted into installing portable memory contracts.
    #[error("factory does not support explicit memory bindings")]
    MemoryBindingUnsupported,
    /// Attached services have been dropped.
    #[error("hivemind host is unavailable")]
    Unavailable,
    /// Durable registration has not completed; retry registration first.
    #[error("agent registration is pending")]
    RegistrationPending,
    /// Host explicitly denied management.
    #[error("management denied: {0}")]
    Unauthorized(String),
    /// The host send policy refused an outbound tool call.
    #[error("send denied: {0}")]
    SendDenied(String),
    /// Shared adapter state was poisoned.
    #[error("adapter lock poisoned")]
    Poisoned,
    /// A hive memory root `OpenHuman` would not accept, or one that isolates
    /// nothing.
    #[error("invalid memory root {root:?}: {reason}")]
    InvalidMemoryRoot {
        /// The refused root.
        root: String,
        /// Why it was refused.
        reason: String,
    },
    /// A seat id that cannot be a memory agent id verbatim.
    #[error("invalid memory agent id {agent_id:?}: {reason}")]
    InvalidMemoryAgentId {
        /// The refused id.
        agent_id: String,
        /// Why it was refused.
        reason: String,
    },
    /// Hive memory is on and a registered seat was built without its binding.
    #[error("seat {seat} is not bound to the hive memory: {reason}")]
    UnboundSeat {
        /// The seat's agent id.
        seat: String,
        /// What differs from the hive's binding.
        reason: String,
    },
    /// `OpenHuman` has no memory engine bound for the config (memory is off,
    /// or the engine's credential is missing).
    #[error(transparent)]
    Memory(#[from] openhuman_core::memory::MemoryError),
    /// A turn timeout of zero would fail every turn before it starts.
    #[error("turn timeout must be nonzero")]
    InvalidTurnTimeout,
    /// A failed `replace_agent` left the agent without a live handle.
    #[error("agent {0} has no live handle; retry replace_agent")]
    NoHandle(String),
    /// Agent turn exceeded the wall.
    #[error("agent turn timed out")]
    TimedOut,
    /// Coordinator refused the operation.
    #[error(transparent)]
    Coordinator(#[from] tinyhivemind_hives::Error),
    /// The host factory could not instantiate its configured agent.
    #[error(transparent)]
    Agent(#[from] openhuman_embed::AgentError),
    /// Named attachment refused the tools.
    #[error(transparent)]
    Attachment(#[from] openhuman_embed::ToolAttachmentError),
    /// Tool argument decoding failed.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// `OpenHuman` or the host hook failed.
    #[error(transparent)]
    Harness(#[from] anyhow::Error),
}
