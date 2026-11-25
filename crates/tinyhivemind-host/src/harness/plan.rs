//! What a harness is given to start a seat.
use super::{Approver, DenyAll, Ignore, Observer};
use std::collections::BTreeSet;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tinyhivemind_lang::LoweredSeat;

/// A seat to start.
///
/// The seat itself is the package's own lowered one, with its id, prompt,
/// model, limits and settings (`config`); a client reads what its harness can
/// honour and refuses the rest. Only what a package cannot say is added.
#[derive(Clone)]
pub struct SeatPlan {
    /// The seat, lowered from its package.
    pub seat: LoweredSeat,
    /// An absolute path, private to this seat, that survives restarts. The same
    /// path is what lets a session be resumed.
    ///
    /// With one instance per seat this is the instance's directory. A client
    /// whose seats share an instance keeps that in a directory it was given
    /// when it was constructed, and only what is this seat's alone in here, so
    /// two seats never have the same one. Such an instance holds every one of
    /// its seats' credentials, and stopping one seat leaves it running.
    pub directory: PathBuf,
    /// Told what the seat does.
    pub observer: Arc<dyn Observer>,
    /// Asked about the harness's own tools.
    pub approver: Arc<dyn Approver>,
}

impl SeatPlan {
    /// Start `seat` in `directory`, observing nothing and approving nothing.
    #[must_use]
    pub fn new(seat: LoweredSeat, directory: impl Into<PathBuf>) -> Self {
        Self {
            seat,
            directory: directory.into(),
            observer: Arc::new(Ignore),
            approver: Arc::new(DenyAll),
        }
    }

    /// Longest one turn may take, if the seat is limited.
    #[must_use]
    pub fn turn_timeout(&self) -> Option<Duration> {
        let limit = self.seat.integration.limits.timeout_ms;
        limit.map(Duration::from_millis)
    }
}

impl fmt::Debug for SeatPlan {
    /// The prompt and settings are long, and the hive's business.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SeatPlan")
            .field("seat", &self.seat.id)
            .field("directory", &self.directory)
            .finish_non_exhaustive()
    }
}

/// Where a seat's hive tools are served, and the proof of who is calling.
#[derive(Clone, Eq, PartialEq)]
pub struct ToolEndpoint {
    /// The server's name, which harnesses put in the names of its tools.
    pub server: String,
    /// Its URL.
    pub url: String,
    /// The tools it serves, by the names it serves them under.
    pub tools: BTreeSet<String>,
    bearer: String,
}

impl ToolEndpoint {
    /// `tools` served by `server` at `url` to whoever presents `bearer`.
    #[must_use]
    pub fn new(
        server: impl Into<String>,
        url: impl Into<String>,
        bearer: impl Into<String>,
        tools: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            server: server.into(),
            url: url.into(),
            tools: tools.into_iter().collect(),
            bearer: bearer.into(),
        }
    }

    /// The seat's secret. It belongs in the instance's private configuration
    /// and nowhere else: not in an argument, a log or an error.
    #[must_use]
    pub fn bearer(&self) -> &str {
        &self.bearer
    }
}

impl fmt::Debug for ToolEndpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToolEndpoint")
            .field("server", &self.server)
            .field("url", &self.url)
            .field("tools", &self.tools)
            .field("bearer", &"redacted")
            .finish()
    }
}
