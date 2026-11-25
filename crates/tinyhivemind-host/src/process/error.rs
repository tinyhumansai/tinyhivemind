//! Why an instance could not be started, readied or stopped.
use super::Exit;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

/// Result of a supervision call.
pub type Result<T> = std::result::Result<T, Error>;

/// Supervision failures. None of them repeats an environment value, an
/// argument or the child's output, any of which may hold a secret.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The launch itself is not acceptable.
    #[error("invalid launch: {0}")]
    Launch(&'static str),
    /// A directory of the instance could not be made or read.
    #[error("instance directory {path}: {source}")]
    Directory {
        /// The directory.
        path: PathBuf,
        /// The failure.
        source: io::Error,
    },
    /// The path is not a directory of this user's own.
    #[error("{path} is not a directory owned by this user")]
    NotPrivate {
        /// The refused path.
        path: PathBuf,
    },
    /// The program could not be started.
    #[error("could not start {program}: {source}")]
    Spawn {
        /// The program.
        program: PathBuf,
        /// The failure.
        source: io::Error,
    },
    /// The child exited before it was ready.
    #[error("the child exited before it was ready: {0}")]
    Exited(Exit),
    /// The readiness probe failed.
    #[error("the child is not ready: {0}")]
    NotReady(String),
    /// The child was not ready in time.
    #[error("the child was not ready within {deadline:?}")]
    ReadyTimeout {
        /// The bound that passed.
        deadline: Duration,
    },
    /// The process table could not be read, so nothing is known of the tree.
    #[error("the process table is unreadable: {0}")]
    Table(#[source] io::Error),
    /// Processes of the instance were still there when the time ran out. The
    /// instance is not stopped.
    #[error("{} processes of the instance are still running: {pids:?}", pids.len())]
    Survivors {
        /// Their pids. In [`super::Containment::ProcessGroup`] this is the
        /// group's id alone.
        pids: Vec<u32>,
    },
    /// The supervisor thread is gone without having proven the tree stopped.
    #[error("the supervisor of the instance is gone")]
    Unsupervised,
}
