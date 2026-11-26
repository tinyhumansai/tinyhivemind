//! [`OpenHumanHarness`]: the `openhuman` kind of the host's harness port.
//!
//! Each seat is one instance of the pinned binary, started by
//! [`crate::instance`] and run by a seat runner held to the port's rules by
//! [`SeatRunner`]. See the crate documentation for what a seat may ask for.
mod seat;

use crate::capability;
use seat::{Runner, Seat};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tinyhivemind_hives::AgentRunner;
use tinyhivemind_host::harness::{
    Error, Harness, HarnessKind, SeatPlan, SeatRunner, StartFuture, StopFuture, ToolEndpoint,
};
use tinyhivemind_host::process::StopPolicy;

/// The pinned `openhuman-core` binary. The library never downloads it:
/// `.github/scripts/fetch-harness.sh` fetches and checks the one in
/// `harnesses.lock`, and the host passes its path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Program {
    /// The binary, an absolute path.
    pub path: PathBuf,
    /// The version it must report, as `harnesses.lock` records it.
    pub version: String,
}

impl Program {
    /// The binary at `path`, which must report `version`.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>, version: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            version: version.into(),
        }
    }
}

/// How long the steps of starting and stopping an instance may take.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Options {
    /// From spawn to a healthy `/health`.
    pub ready: Duration,
    /// For the instance to list the host's tools, which it does only once
    /// it has connected to the MCP server.
    pub warm_up: Duration,
    /// Each step of a stop.
    pub stop: StopPolicy,
}

impl Default for Options {
    /// 30 s to be healthy, 20 s to warm up, and the supervisor's own stop.
    fn default() -> Self {
        Self {
            ready: Duration::from_secs(30),
            warm_up: Duration::from_secs(20),
            stop: StopPolicy::default(),
        }
    }
}

/// Runs seats on `OpenHuman`, one supervised `openhuman-core` per seat.
///
/// Its futures must be driven within a Tokio runtime with time and I/O
/// enabled.
#[derive(Debug)]
pub struct OpenHumanHarness {
    program: Program,
    options: Options,
    seats: Arc<Mutex<BTreeMap<String, Arc<Seat>>>>,
}

impl OpenHumanHarness {
    /// Seats on `program`, with the default [`Options`].
    #[must_use]
    pub fn new(program: Program) -> Self {
        Self {
            program,
            options: Options::default(),
            seats: Arc::default(),
        }
    }

    /// The same, with `options`.
    #[must_use]
    pub fn with_options(mut self, options: Options) -> Self {
        self.options = options;
        self
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Harness for OpenHumanHarness {
    fn kind(&self) -> HarnessKind {
        HarnessKind::OpenHuman
    }

    /// Refuses, with [`Error::Refused`] holding a [`capability::Refusal`], a
    /// seat this harness cannot honour, before anything is started.
    fn start(&self, plan: SeatPlan, tools: ToolEndpoint) -> StartFuture {
        let program = self.program.clone();
        let options = self.options;
        let seats = self.seats.clone();
        Box::pin(async move {
            let model = capability::check(&plan.seat)
                .map_err(|refusal| Error::Refused(Box::new(refusal)))?;
            let id = plan.seat.id.clone();
            if lock(&seats).contains_key(&id) {
                return Err(Error::Harness(format!("seat {id} is already running")));
            }
            let timeout = plan.turn_timeout();
            let seat = Arc::new(Seat::new(program, options, plan, tools, model));
            seat.start().await?;
            lock(&seats).insert(id, seat.clone());
            let runner = SeatRunner::new(Arc::new(Runner(seat))).with_timeout(timeout);
            Ok(Arc::new(runner) as Arc<dyn AgentRunner>)
        })
    }

    fn stop(&self, seat_id: &str) -> StopFuture {
        let seat = lock(&self.seats).remove(seat_id);
        Box::pin(async move {
            match seat {
                Some(seat) => Ok(seat.stop().await?),
                None => Ok(()),
            }
        })
    }
}
