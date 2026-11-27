//! What can stop a hive's seats from starting or stopping.
use tinyhivemind_acp::CatalogError;
use tinyhivemind_host::harness;
use tinyhivemind_lang::HarnessKind;

/// Result of this crate's fallible steps.
pub type Result<T> = std::result::Result<T, Error>;

/// Why seats could not be started or stopped.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The seat's harness kind needs an install the host did not give.
    #[error("seat {seat} runs on {kind}, but no {missing} is installed")]
    MissingInstall {
        /// The seat.
        seat: String,
        /// Its harness kind.
        kind: HarnessKind,
        /// What is missing.
        missing: &'static str,
    },
    /// The seat names no model, or no endpoint for it.
    #[error("seat {seat} names no model endpoint")]
    NoModel {
        /// The seat.
        seat: String,
    },
    /// Two `OpenClaw` seats share a Gateway but not a model: a Gateway has one
    /// provider.
    #[error(
        "openclaw seats {first} and {seat} share a gateway but not a model endpoint; give one of them isolation seat"
    )]
    SharedGateway {
        /// The seat that set the Gateway's model.
        first: String,
        /// The seat that differs.
        seat: String,
    },
    /// No randomness for a Gateway token.
    #[error("a gateway token could not be generated: {0}")]
    Random(String),
    /// A directory or file of the deployment could not be made.
    #[error("{what}: {source}")]
    Io {
        /// What was being made.
        what: String,
        /// Why it failed.
        source: std::io::Error,
    },
    /// A Codex seat's model catalog could not be written.
    #[error("seat {seat}'s codex catalog: {source}")]
    Catalog {
        /// The seat.
        seat: String,
        /// Why it failed.
        source: CatalogError,
    },
    /// The hive tool server failed.
    #[error("the tool server: {0}")]
    Server(#[from] tinyhivemind_mcp::Error),
    /// An `OpenClaw` Gateway did not start or stop.
    #[error("the openclaw gateway {gateway}: {source}")]
    Gateway {
        /// The Gateway's directory name.
        gateway: String,
        /// Why it failed.
        source: harness::Error,
    },
    /// A seat's harness did not start it.
    #[error("seat {seat} did not start: {source}")]
    Start {
        /// The seat.
        seat: String,
        /// Why it failed.
        source: harness::Error,
    },
    /// A started seat could not be registered with the coordinator.
    #[error("seat {seat} could not be registered: {source}")]
    Register {
        /// The seat.
        seat: String,
        /// Why it failed.
        source: tinyhivemind_hives::Error,
    },
    /// A seat's harness did not stop it.
    #[error("seat {seat} did not stop: {source}")]
    Stop {
        /// The seat.
        seat: String,
        /// Why it failed.
        source: harness::Error,
    },
}

impl Error {
    /// An I/O failure while making `what`.
    pub(crate) fn io(what: impl Into<String>) -> impl FnOnce(std::io::Error) -> Self {
        let what = what.into();
        move |source| Self::Io { what, source }
    }
}

#[cfg(test)]
mod test;
