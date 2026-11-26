//! Typed failures of hive tool calls and management.
/// Host result.
pub type Result<T> = std::result::Result<T, Error>;

/// Failures dispatching a hive tool or a management request.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A tool's arguments do not fit its declaration.
    #[error(transparent)]
    Arguments(#[from] ArgumentError),
    /// Management was not configured by the host.
    #[error("management is disabled")]
    ManagementDisabled,
    /// Host explicitly denied management.
    #[error("management denied: {0}")]
    Unauthorized(String),
    /// The host send policy refused an outbound tool call.
    #[error("send denied: {0}")]
    SendDenied(String),
    /// Coordinator refused the operation.
    #[error(transparent)]
    Coordinator(#[from] tinyhivemind_hives::Error),
    /// Tool argument decoding or result encoding failed.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// A host-supplied policy or agent creator failed with its own error.
    #[error(transparent)]
    Host(Box<dyn std::error::Error + Send + Sync>),
}

/// Why a tool's arguments were refused before it ran.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ArgumentError {
    /// Arguments are a JSON value other than an object.
    #[error("arguments must be an object")]
    NotAnObject,
    /// A key the tool does not declare.
    #[error("unknown argument {0}")]
    Unknown(String),
    /// A required field is absent.
    #[error("missing argument {0}")]
    Missing(&'static str),
    /// A field holds the wrong JSON type.
    #[error("invalid {name}: expected {expected}")]
    Invalid {
        /// The declared field.
        name: &'static str,
        /// Its declared type.
        expected: &'static str,
    },
    /// A read named both destinations or neither.
    #[error("supply exactly one of hive_id or agent_id")]
    OneDestination,
    /// A direct read named a hive thread.
    #[error("thread is only valid for hive reads")]
    ThreadOnDirectRead,
}

#[cfg(test)]
mod test;
