//! Why a seat could not be started, run or stopped.
use super::Mismatch;
use std::time::Duration;

/// Result of a port call.
pub type Result<T> = std::result::Result<T, Error>;

/// Port failures.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The instance's process could not be supervised.
    #[cfg(unix)]
    #[error(transparent)]
    Process(#[from] crate::process::Error),
    /// The started instance is not what was asked for.
    #[error(transparent)]
    Mismatch(#[from] Mismatch),
    /// The harness refused a call, broke its protocol or went away.
    #[error("harness failed: {0}")]
    Harness(String),
    /// The harness no longer has the seat's session. A new one is never
    /// started in its place.
    #[error("session {session_id} could not be resumed: {reason}")]
    Resume {
        /// The session the seat has.
        session_id: String,
        /// What the harness said.
        reason: String,
    },
    /// The turn ran past the seat's limit and was cancelled.
    #[error("the turn did not end within {0:?}")]
    TimedOut(Duration),
    /// The request names no turn, so a replay of it could not be told.
    #[error("the request has no turn id")]
    NoTurnId,
    /// The turn already ran, or may have. Its outcome is for the coordinator
    /// to settle; it is never run again.
    #[error("turn {0} already started")]
    Replayed(String),
    /// The turn ended without a session to continue.
    #[error("the harness returned an empty session id")]
    EmptySession,
    /// The seat has one session for life, and this is not it.
    #[error("the seat's session is {expected}, not {found}")]
    SessionChanged {
        /// The seat's session.
        expected: String,
        /// The one that was asked for or returned.
        found: String,
    },
    /// An earlier turn may still be running, so the seat admits no other.
    #[error("turn {turn_id} could not be shown to have stopped: {reason}{}", ended_in(.ended.as_deref()))]
    Unsettled {
        /// The turn in doubt.
        turn_id: String,
        /// Why its cancellation failed.
        reason: String,
        /// How the turn itself ended, if it said.
        ended: Option<String>,
    },
}

fn ended_in(ended: Option<&str>) -> String {
    ended.map_or_else(String::new, |ended| format!(", after it ended in: {ended}"))
}

/// The coordinator records a runner's failure by its message.
impl From<Error> for tinyhivemind_hives::Error {
    fn from(error: Error) -> Self {
        Self::InvalidState(format!("harness turn failed: {error}"))
    }
}
