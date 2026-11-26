//! What can go wrong in this crate, and how the port sees it.
use crate::capability::Refusal;
use crate::connection;
use tinyhivemind_host::harness::{self, Mismatch};
use tinyhivemind_host::process;

/// Result of this crate's fallible steps.
pub(crate) type Result<T> = std::result::Result<T, Error>;

/// Failures before they reach the port.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// The seat asks for what this agent cannot do.
    #[error(transparent)]
    Refused(#[from] Refusal),
    /// The agent's protocol failed.
    #[error(transparent)]
    Protocol(#[from] connection::Error),
    /// The process could not be supervised.
    #[error(transparent)]
    Process(#[from] process::Error),
    /// The agent is not the one asked for.
    #[error(transparent)]
    Mismatch(#[from] Mismatch),
    /// The seat's session could not be resumed; no new one is started.
    #[error("session {session} could not be resumed: {reason}")]
    Resume { session: String, reason: String },
    /// The turn's request could not be rendered as its prompt.
    #[error("the turn could not be rendered: {0}")]
    Render(String),
    /// The seat was stopped.
    #[error("the seat is stopped")]
    Closed,
    /// A file of the instance could not be written.
    #[error("{what}: {source}")]
    Io {
        what: &'static str,
        source: std::io::Error,
    },
}

impl Error {
    /// An I/O failure at `what`.
    pub(crate) fn io(what: &'static str) -> impl FnOnce(std::io::Error) -> Self {
        move |source| Self::Io { what, source }
    }
}

impl From<Error> for harness::Error {
    fn from(error: Error) -> Self {
        match error {
            Error::Refused(refusal) => Self::Refused(Box::new(refusal)),
            Error::Process(error) => Self::Process(error),
            Error::Mismatch(mismatch) => Self::Mismatch(mismatch),
            Error::Resume { session, reason } => Self::Resume {
                session_id: session,
                reason,
            },
            other => Self::Harness(other.to_string()),
        }
    }
}

#[cfg(test)]
mod test;
