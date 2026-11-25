//! Typed boundary and persistence failures.
/// Coordinator operation result.
pub type Result<T> = std::result::Result<T, Error>;
/// Validation, storage, and conductor failures.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Empty or reserved identifier.
    #[error("invalid {0}")]
    InvalidIdentifier(&'static str),
    /// Invalid scheduler bounds.
    #[error("round width and conductor walls must be nonzero")]
    InvalidOptions,
    /// Registration belongs to another live runtime.
    #[error("agent belongs to another runtime")]
    RuntimeMismatch,
    /// A continuing session cannot be replaced after binding or claim.
    #[error("conflicting session binding for {0}")]
    SessionConflict(String),
    /// Existing live handle differs from the registration.
    #[error("conflicting registration for {0}")]
    AgentConflict(String),
    /// Agent has not been registered durably.
    #[error("unknown agent {0}")]
    UnknownAgent(String),
    /// Hive has not been created.
    #[error("unknown hive {0}")]
    UnknownHive(String),
    /// Same hive ID has another definition.
    #[error("conflicting hive {0}")]
    HiveConflict(String),
    /// Duplicate membership in a supplied definition.
    #[error("duplicate hive member {0}")]
    DuplicateMember(String),
    /// No recipient can receive the hive message.
    #[error("empty hive {0}")]
    EmptyHive(String),
    /// Sender lacks membership or recipient visibility.
    #[error("agent {agent_id} cannot access hive {hive_id}")]
    NotMember {
        /// Agent requesting access.
        agent_id: String,
        /// Requested hive.
        hive_id: String,
    },
    /// Retry ID reused with changed payload.
    #[error("conflicting message {0}")]
    MessageConflict(String),
    /// Conversation root does not belong to the accessible destination.
    #[error("invalid or inaccessible thread {0}")]
    InvalidThread(u64),
    /// No active bound episode turn admits the action.
    #[error("stale episode {0}")]
    StaleEpisode(String),
    /// An identified call names a turn that is not its agent's running one,
    /// or its agent has no running turn.
    #[error("turn {turn_id} is not the running turn of {agent_id}")]
    StaleTurn {
        /// Agent the call was made as.
        agent_id: String,
        /// Turn the call named.
        turn_id: String,
    },
    /// An identified call reuses a `call` its turn already made with another
    /// tool or other arguments.
    #[error("call {0} was already made with another tool or arguments")]
    CallReused(String),
    /// Store revision changed before commit.
    #[error("storage revision conflict: expected {expected}, found {actual}")]
    RevisionConflict {
        /// Supplied revision.
        expected: u64,
        /// Current stored revision.
        actual: u64,
    },
    /// This coordinator's epoch is lower than the store's; a new process has
    /// taken ownership. No further writes are possible.
    #[error("coordinator fenced: writer epoch {coordinator} < stored {stored}")]
    Fenced {
        /// This coordinator's epoch.
        coordinator: u64,
        /// The epoch currently owning the store.
        stored: u64,
    },
    /// The recipient already holds the most pending direct messages
    /// [`crate::RetentionPolicy::pending_per_agent`] allows.
    #[error("inbox of {agent_id} is full ({limit} pending)")]
    InboxFull {
        /// The recipient whose inbox is full.
        agent_id: String,
        /// The configured bound.
        limit: usize,
    },
    /// An appended transcript row does not follow the stored transcript.
    #[error("transcript row {0} does not extend the stored transcript")]
    TranscriptOutOfOrder(u64),
    /// Next snapshot does not advance exactly one revision.
    #[error("invalid next storage revision")]
    InvalidRevision,
    /// Arithmetic would exhaust the sequence or revision space.
    #[error("sequence or revision exhausted")]
    Exhausted,
    /// A shared lock was poisoned.
    #[error("coordinator lock poisoned")]
    Poisoned,
    /// Snapshot lacks a required conductor state.
    #[error("invalid durable snapshot: {0}")]
    InvalidState(String),
    /// Core conductor rejected a transition.
    #[error(transparent)]
    Conductor(#[from] tinyhivemind_core::driver::Error),
    /// Snapshot wire representation failed.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// `SQLite` operation failed.
    #[cfg(feature = "sqlite")]
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}
