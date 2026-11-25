//! One turn at a time, one session for life, and no turn twice.
use super::{Error, Result};
use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tinyhivemind_hives::{AgentRunner, TurnFuture, TurnOutcome, TurnRequest};
use tokio::sync::{Mutex, OwnedMutexGuard};

/// Future returned by [`CancellableRunner::cancel`].
pub type CancelFuture = Pin<Box<dyn Future<Output = Result<()>> + Send>>;

/// A client's own runner, before [`SeatRunner`] is put around it.
pub trait CancellableRunner: AgentRunner {
    /// Stop the turn `turn_id`, returning once it is *shown* to have stopped:
    /// the harness said so, or its instance was killed and is gone.
    ///
    /// Called when a run failed, ran out of time or was dropped. The turn may
    /// never have reached the harness, may already have ended, and may be
    /// cancelled more than once; all of those succeed. The wait must be
    /// bounded, since the seat admits no turn until it is over.
    fn cancel(&self, turn_id: &str) -> CancelFuture;
}

/// The runner a client registers for a seat.
///
/// The coordinator already reserves one turn per agent and compares a returned
/// session with the one it asked for. What it cannot see is the harness, so
/// this holds what that leaves open:
///
/// - **One turn at a time.** Dropping a drain ends the reservation at once,
///   and the next drain may claim the same agent while the harness is still
///   winding down. A run waits here until the previous turn is settled.
/// - **Settled.** A run that fails, runs out of time or is dropped is
///   cancelled. If that cannot be shown to have worked, the seat admits no
///   turn until a further cancellation does.
/// - **One session for life.** Also when the coordinator asks for none, which
///   it does if it was interrupted before it stored the first. A request for
///   another session never reaches the harness.
/// - **No turn twice.** The coordinator never reuses a turn id, but a caller
///   that retries a failed run would, and no harness has a durable idempotency
///   key. An id that ever started here is refused. They are kept for the life
///   of the runner.
pub struct SeatRunner {
    inner: Arc<dyn CancellableRunner>,
    timeout: Option<Duration>,
    seat: Arc<Mutex<Seat>>,
}

#[derive(Default)]
struct Seat {
    session: Option<String>,
    started: HashSet<String>,
    /// A turn not yet shown to have ended.
    unsettled: Option<String>,
}

impl SeatRunner {
    /// Hold `inner` to the rules.
    #[must_use]
    pub fn new(inner: Arc<dyn CancellableRunner>) -> Self {
        Self {
            inner,
            timeout: None,
            seat: Arc::default(),
        }
    }

    /// Cancel a turn that runs longer than `timeout`.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.timeout = timeout;
        self
    }
}

impl std::fmt::Debug for SeatRunner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SeatRunner")
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

impl AgentRunner for SeatRunner {
    fn run(&self, request: TurnRequest) -> TurnFuture {
        let inner = self.inner.clone();
        let timeout = self.timeout;
        let seat = self.seat.clone();
        Box::pin(async move {
            let mut seat = seat.lock_owned().await;
            let request = admit(inner.as_ref(), &mut seat, request).await?;
            let turn = Turn {
                id: request.turn_id.clone(),
                inner,
                seat: Some(seat),
            };
            turn.run(request, timeout).await
        })
    }
}

/// Let `request` start a turn, with the seat's session, or say why not.
async fn admit(
    inner: &dyn CancellableRunner,
    seat: &mut Seat,
    mut request: TurnRequest,
) -> Result<TurnRequest> {
    if let Some(turn_id) = seat.unsettled.clone() {
        settle(inner, seat, turn_id).await?;
    }
    if request.turn_id.is_empty() {
        return Err(Error::NoTurnId);
    }
    if seat.started.contains(&request.turn_id) {
        return Err(Error::Replayed(request.turn_id));
    }
    match (&seat.session, &request.session_id) {
        (Some(expected), Some(found)) => same(expected, found)?,
        (Some(session), None) => request.session_id = Some(session.clone()),
        (None, Some(session)) => seat.session = Some(session.clone()),
        (None, None) => {}
    }
    seat.started.insert(request.turn_id.clone());
    seat.unsettled = Some(request.turn_id.clone());
    Ok(request)
}

/// Cancel `turn_id` and clear it, or leave it in doubt.
async fn settle(inner: &dyn CancellableRunner, seat: &mut Seat, turn_id: String) -> Result<()> {
    match inner.cancel(&turn_id).await {
        Ok(()) => {
            seat.unsettled = None;
            Ok(())
        }
        Err(error) => Err(Error::Unsettled {
            turn_id,
            reason: error.to_string(),
        }),
    }
}

fn same(expected: &str, found: &str) -> Result<()> {
    if expected == found {
        return Ok(());
    }
    Err(Error::SessionChanged {
        expected: expected.into(),
        found: found.into(),
    })
}

/// A started turn, which holds the seat until it is settled. Dropping it
/// unsettled cancels it, still holding the seat.
struct Turn {
    id: String,
    inner: Arc<dyn CancellableRunner>,
    seat: Option<OwnedMutexGuard<Seat>>,
}

impl Turn {
    async fn run(
        mut self,
        request: TurnRequest,
        timeout: Option<Duration>,
    ) -> tinyhivemind_hives::Result<TurnOutcome> {
        let run = self.inner.run(request);
        let ended = match timeout {
            Some(limit) => tokio::time::timeout(limit, run)
                .await
                .unwrap_or_else(|_| Err(Error::TimedOut(limit).into())),
            None => run.await,
        };
        match ended {
            Ok(outcome) => {
                self.keep(&outcome.session_id)?;
                Ok(outcome)
            }
            Err(error) => {
                self.settle().await?;
                Err(error)
            }
        }
    }

    /// The turn ended by itself, in `session`.
    fn keep(&mut self, session: &str) -> Result<()> {
        let Some(mut seat) = self.seat.take() else {
            return Ok(());
        };
        seat.unsettled = None;
        if session.trim().is_empty() {
            return Err(Error::EmptySession);
        }
        match &seat.session {
            Some(expected) => same(expected, session),
            None => {
                seat.session = Some(session.into());
                Ok(())
            }
        }
    }

    /// The turn ended in an error, which does not say that it stopped.
    async fn settle(&mut self) -> Result<()> {
        if let Some(seat) = &mut self.seat {
            // Dropped while waiting here, the turn is cancelled again.
            let settled = settle(self.inner.as_ref(), seat, self.id.clone()).await;
            self.seat = None;
            return settled;
        }
        Ok(())
    }
}

impl Drop for Turn {
    fn drop(&mut self) {
        let Some(mut seat) = self.seat.take() else {
            return;
        };
        // Without a runtime the seat keeps the doubt, and the next turn
        // settles it first.
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let inner = self.inner.clone();
            let id = std::mem::take(&mut self.id);
            runtime.spawn(async move {
                // A failure leaves the same doubt.
                let _ = settle(inner.as_ref(), &mut seat, id).await;
            });
        }
    }
}
