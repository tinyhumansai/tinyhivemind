//! One turn at a time, one session for life, and no turn twice.
use super::{Error, Result};
use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tinyhivemind_hives::{AgentRunner, TurnDisposition, TurnFuture, TurnOutcome, TurnRequest};
use tokio::sync::{Mutex, OwnedMutexGuard};

/// How long a cancellation may take. Enough to kill an instance and prove it.
const CANCEL_LIMIT: Duration = Duration::from_secs(30);

/// Future returned by [`CancellableRunner::cancel`].
pub type CancelFuture = Pin<Box<dyn Future<Output = Result<()>> + Send>>;

/// A client's own runner, before [`SeatRunner`] is put around it.
pub trait CancellableRunner: AgentRunner {
    /// Stop the turn `turn_id`, returning once it is *shown* to have stopped:
    /// the harness said so, or its instance was killed and is gone.
    ///
    /// Called when a run failed, ran out of time or was dropped. The turn may
    /// never have reached the harness, may already have ended, and may be
    /// cancelled more than once; all of those succeed. So does cancelling a
    /// turn of an instance that was stopped since. The future is dropped if it
    /// takes longer than [`SeatRunner::with_cancel_limit`] allows, and the turn
    /// then stays in doubt.
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
/// - **Settled.** A run that fails, runs out of time, is dropped or reports
///   [`TurnDisposition::Failed`] is cancelled. If that cannot be shown to have
///   worked, each further turn tries again first and is refused with
///   [`Error::Unsettled`] if it cannot either. The way out is to stop the
///   instance, with [`super::Harness::stop`], and start the seat again or, if
///   this runner is kept, call [`SeatRunner::instance_stopped`].
/// - **Parked is ended.** [`TurnDisposition::Parked`] promises, like
///   [`TurnDisposition::Completed`], that the harness's own turn is over: the
///   release comes as another turn with another id. A client must not return
///   it while the harness waits for an answer.
/// - **One session for life.** Also when the coordinator asks for none, which
///   it does if it was interrupted before it stored the first. A request for
///   another session never reaches the harness.
/// - **No turn twice.** The coordinator never reuses a turn id, but a caller
///   that retries a failed run would, and no harness has a durable idempotency
///   key. An id that ever started here is refused. All of them are kept for
///   the life of the runner, about 100 bytes each, since forgetting any would
///   allow it to run again.
///
/// Its futures must be driven within a Tokio runtime with time enabled, as must
/// those of every client that supervises a process.
pub struct SeatRunner {
    inner: Arc<dyn CancellableRunner>,
    limits: TurnLimits,
    seat: Arc<Mutex<Seat>>,
}

#[derive(Clone, Copy, Debug)]
struct TurnLimits {
    run: Option<Duration>,
    cancel: Duration,
}

#[derive(Default)]
struct Seat {
    session: Option<String>,
    started: HashSet<String>,
    /// A turn not yet shown to have ended, and how it ended if it said.
    unsettled: Option<(String, Option<String>)>,
}

impl SeatRunner {
    /// Hold `inner` to the rules.
    #[must_use]
    pub fn new(inner: Arc<dyn CancellableRunner>) -> Self {
        Self {
            inner,
            limits: TurnLimits {
                run: None,
                cancel: CANCEL_LIMIT,
            },
            seat: Arc::default(),
        }
    }

    /// Cancel a turn that runs longer than `timeout`.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.limits.run = timeout;
        self
    }

    /// Give a cancellation `limit` instead of 30 seconds.
    #[must_use]
    pub fn with_cancel_limit(mut self, limit: Duration) -> Self {
        self.limits.cancel = limit;
        self
    }

    /// The instance that ran `turn_id`, the one an [`Error::Unsettled`] names,
    /// was proven stopped since, so that the turn is over and the seat may go
    /// on. Says whether that was the turn in doubt: a later one, of a later
    /// instance, is not settled by this. Waits for a running turn or
    /// cancellation to notice.
    ///
    /// Calling this without that proof gives up the guarantee.
    pub async fn instance_stopped(&self, turn_id: &str) -> bool {
        let mut seat = self.seat.lock().await;
        let settled = seat.unsettled.as_ref().is_some_and(|(id, _)| id == turn_id);
        if settled {
            seat.unsettled = None;
        }
        settled
    }
}

impl std::fmt::Debug for SeatRunner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SeatRunner")
            .field("limits", &self.limits)
            .finish_non_exhaustive()
    }
}

impl AgentRunner for SeatRunner {
    fn run(&self, request: TurnRequest) -> TurnFuture {
        let inner = self.inner.clone();
        let limits = self.limits;
        let seat = self.seat.clone();
        Box::pin(async move {
            let mut seat = seat.lock_owned().await;
            let request = admit(inner.as_ref(), limits, &mut seat, request).await?;
            let turn = Turn {
                id: request.turn_id.clone(),
                inner,
                limits,
                seat: Some(seat),
            };
            turn.run(request).await
        })
    }
}

/// Let `request` start a turn, with the seat's session, or say why not.
async fn admit(
    inner: &dyn CancellableRunner,
    limits: TurnLimits,
    seat: &mut Seat,
    mut request: TurnRequest,
) -> Result<TurnRequest> {
    if seat.unsettled.is_some() {
        settle(inner, limits, seat).await?;
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
    seat.unsettled = Some((request.turn_id.clone(), None));
    Ok(request)
}

/// Cancel the turn in doubt and clear it, or leave it in doubt.
async fn settle(inner: &dyn CancellableRunner, limits: TurnLimits, seat: &mut Seat) -> Result<()> {
    let Some((turn_id, ended)) = seat.unsettled.clone() else {
        return Ok(());
    };
    let limit = limits.cancel;
    let reason = match tokio::time::timeout(limit, inner.cancel(&turn_id)).await {
        Ok(Ok(())) => {
            seat.unsettled = None;
            return Ok(());
        }
        Ok(Err(error)) => error.to_string(),
        Err(_) => format!("its cancellation did not end within {limit:?}"),
    };
    Err(Error::Unsettled {
        turn_id,
        reason,
        ended,
    })
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
    limits: TurnLimits,
    seat: Option<OwnedMutexGuard<Seat>>,
}

impl Turn {
    async fn run(mut self, request: TurnRequest) -> tinyhivemind_hives::Result<TurnOutcome> {
        let run = self.inner.run(request);
        let ended = match self.limits.run {
            Some(limit) => tokio::time::timeout(limit, run)
                .await
                .unwrap_or_else(|_| Err(Error::TimedOut(limit).into())),
            None => run.await,
        };
        let outcome = match ended {
            Ok(outcome) => outcome,
            Err(error) => {
                self.settle().await?;
                return Err(error);
            }
        };
        self.keep(&outcome.session_id)?;
        match outcome.disposition {
            // Says that the turn failed, not that the harness is done with it.
            TurnDisposition::Failed(_) => self.settle().await?,
            TurnDisposition::Completed | TurnDisposition::Parked => self.end(),
        }
        Ok(outcome)
    }

    /// The turn ended, in whatever session it names.
    fn end(&mut self) {
        if let Some(mut seat) = self.seat.take() {
            seat.unsettled = None;
        }
    }

    /// Hold the seat to `session`.
    fn keep(&mut self, session: &str) -> Result<()> {
        let Some(seat) = &mut self.seat else {
            return Ok(());
        };
        if session.trim().is_empty() {
            return Err(Error::EmptySession);
        }
        if let Some(expected) = &seat.session {
            return same(expected, session);
        }
        seat.session = Some(session.into());
        Ok(())
    }

    /// The run ended in a way that does not say that the turn stopped.
    async fn settle(&mut self) -> Result<()> {
        if let Some(seat) = &mut self.seat {
            // Dropped while waiting here, the turn is cancelled again.
            let settled = settle(self.inner.as_ref(), self.limits, seat, self.id.clone()).await;
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
            let limits = self.limits;
            let id = std::mem::take(&mut self.id);
            runtime.spawn(async move {
                // A failure leaves the same doubt.
                let _ = settle(inner.as_ref(), limits, &mut seat, id).await;
            });
        }
    }
}
