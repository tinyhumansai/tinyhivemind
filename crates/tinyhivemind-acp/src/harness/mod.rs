//! [`AcpHarness`]: the host's harness port for any [`Agent`].
//!
//! Each seat is one process of the agent, started by [`crate::seat`] and run
//! by a seat runner held to the port's rules by [`SeatRunner`].
use crate::agent::Agent;
use crate::capability;
use crate::seat::{Runner, Seat};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tinyhivemind_hives::AgentRunner;
use tinyhivemind_host::harness::{
    Error, Harness, HarnessKind, SeatPlan, SeatRunner, StartFuture, StopFuture, ToolEndpoint,
};
use tinyhivemind_host::process::StopPolicy;

/// How long the steps of running a seat may take.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Options {
    /// From spawn to an answered `initialize`.
    pub ready: Duration,
    /// Each step of a stop.
    pub stop: StopPolicy,
    /// For the seat's approver to decide what the agent asks; then the call
    /// is refused.
    pub approval: Duration,
    /// For a cancelled prompt to be answered; then the process is stopped.
    pub cancel: Duration,
}

impl Default for Options {
    /// 60 s to initialize, the supervisor's own stop, 2 minutes for an
    /// approval and 10 s for a cancel.
    fn default() -> Self {
        Self {
            ready: Duration::from_secs(60),
            stop: StopPolicy::default(),
            approval: Duration::from_secs(120),
            cancel: Duration::from_secs(10),
        }
    }
}

/// Runs seats on one ACP agent, one supervised process per seat.
///
/// Its futures must be driven within a Tokio runtime with time and I/O
/// enabled.
#[derive(Debug)]
pub struct AcpHarness {
    agent: Agent,
    options: Options,
    seats: Arc<Mutex<BTreeMap<String, Arc<Seat>>>>,
}

impl AcpHarness {
    /// Seats on `agent`, with the default [`Options`].
    #[must_use]
    pub fn new(agent: Agent) -> Self {
        Self {
            agent,
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

impl Harness for AcpHarness {
    fn kind(&self) -> HarnessKind {
        self.agent.kind
    }

    /// Refuses, with [`Error::Refused`] holding a [`capability::Refusal`], a
    /// seat whose model route the agent cannot take, before anything starts.
    fn start(&self, plan: SeatPlan, tools: ToolEndpoint) -> StartFuture {
        let agent = self.agent.clone();
        let options = self.options;
        let seats = self.seats.clone();
        Box::pin(async move {
            let model = capability::check(&plan.seat, agent.kind)
                .map_err(|refusal| Error::Refused(Box::new(refusal)))?;
            let id = plan.seat.id.clone();
            let timeout = plan.turn_timeout();
            let seat = Arc::new(Seat::new(agent, options, plan, tools, &model));
            {
                // Reserved before anything starts, so that a second start is
                // refused and a stop meanwhile finds the seat to stop.
                let mut seats = lock(&seats);
                if seats.contains_key(&id) {
                    return Err(Error::Harness(format!("seat {id} is already running")));
                }
                seats.insert(id.clone(), seat.clone());
            }
            if let Err(error) = seat.start().await {
                let mut seats = lock(&seats);
                if seats.get(&id).is_some_and(|held| Arc::ptr_eq(held, &seat)) {
                    seats.remove(&id);
                }
                return Err(error.into());
            }
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
