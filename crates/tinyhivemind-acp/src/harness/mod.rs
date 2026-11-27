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

/// The seats of one harness, by id.
type Registry = Arc<Mutex<BTreeMap<String, Arc<Seat>>>>;

/// Runs seats on one ACP agent, one supervised process per seat.
///
/// A seat id is held from the start of its start to the end of its stop, so
/// no two processes ever run for one id: a second start meanwhile is refused.
///
/// Its futures must be driven within a Tokio runtime with time and I/O
/// enabled.
#[derive(Debug)]
pub struct AcpHarness {
    agent: Agent,
    options: Options,
    seats: Registry,
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

/// Free `id` if `seat` still holds it.
fn release(seats: &Registry, id: &str, seat: &Arc<Seat>) {
    let mut seats = lock(seats);
    if seats.get(id).is_some_and(|held| Arc::ptr_eq(held, seat)) {
        seats.remove(id);
    }
}

/// A seat's hold on its id while its start runs. Dropped before
/// [`Self::keep`], because the start failed or its future was dropped, it
/// stops the seat and only then frees the id. A process mid-launch is killed
/// with its dropped instance; one already running is stopped here.
struct Reservation {
    seats: Registry,
    id: String,
    seat: Arc<Seat>,
    kept: bool,
}

impl Reservation {
    /// The start succeeded: the seat keeps its id.
    fn keep(mut self) {
        self.kept = true;
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        if self.kept {
            return;
        }
        let (seats, id, seat) = (self.seats.clone(), self.id.clone(), self.seat.clone());
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn(async move {
                    // A seat that will not stop keeps its id: nothing may
                    // start beside it.
                    if seat.stop().await.is_ok() {
                        release(&seats, &id, &seat);
                    }
                });
            }
            Err(_) => release(&seats, &id, &seat),
        }
    }
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
            let reservation = {
                // Reserved before anything starts, so that a second start is
                // refused and a stop meanwhile finds the seat to stop.
                let mut held = lock(&seats);
                if held.contains_key(&id) {
                    return Err(Error::Harness(format!("seat {id} is already running")));
                }
                held.insert(id.clone(), seat.clone());
                Reservation {
                    seats: seats.clone(),
                    id: id.clone(),
                    seat: seat.clone(),
                    kept: false,
                }
            };
            seat.start().await?;
            reservation.keep();
            let runner = SeatRunner::new(Arc::new(Runner(seat))).with_timeout(timeout);
            Ok(Arc::new(runner) as Arc<dyn AgentRunner>)
        })
    }

    /// The id stays held until the process is proved stopped, and after a
    /// failed stop, so the stop can be retried and nothing starts beside it.
    fn stop(&self, seat_id: &str) -> StopFuture {
        let seats = self.seats.clone();
        let id = seat_id.to_owned();
        let seat = lock(&seats).get(seat_id).cloned();
        Box::pin(async move {
            let Some(seat) = seat else {
                return Ok(());
            };
            seat.stop().await?;
            release(&seats, &id, &seat);
            Ok(())
        })
    }
}
