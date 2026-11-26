//! What can go wrong in this crate, and how the port sees it.
use crate::capability::Refusal;
use crate::{rpc, turn};
use tinyhivemind_host::harness::{self, Mismatch};
use tinyhivemind_host::process;

/// Result of this crate's fallible steps.
pub(crate) type Result<T> = std::result::Result<T, Error>;

/// Failures before they reach the port.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// The seat asks for what this harness cannot do.
    #[error(transparent)]
    Refused(#[from] Refusal),
    /// The instance's API failed.
    #[error(transparent)]
    Api(#[from] rpc::Error),
    /// A turn failed to run.
    #[error(transparent)]
    Turn(#[from] turn::Error),
    /// The process could not be supervised.
    #[error(transparent)]
    Process(#[from] process::Error),
    /// The instance is not the one asked for.
    #[error(transparent)]
    Mismatch(#[from] Mismatch),
    /// The instance never listed some of the host's tools.
    #[error("the instance did not list {missing:?} of server {server}")]
    ToolsNotListed {
        /// The server.
        server: String,
        /// Its tools, by the names it serves them under.
        missing: Vec<String>,
    },
    /// The seat's session is gone from the instance; no new one is started.
    #[error("the instance has no thread {0}")]
    Resume(String),
    /// The turn's request could not be rendered as its message.
    #[error("the turn could not be rendered: {0}")]
    Render(String),
    /// A file of the instance could not be written or read.
    #[error("{what}: {source}")]
    Io {
        /// What it was.
        what: &'static str,
        /// The failure.
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
            Error::Resume(session_id) => Self::Resume {
                reason: format!("the instance has no thread {session_id}"),
                session_id,
            },
            other => Self::Harness(other.to_string()),
        }
    }
}

#[cfg(test)]
mod test;
