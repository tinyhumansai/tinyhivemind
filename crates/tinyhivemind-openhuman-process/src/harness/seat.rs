//! One seat: its instance, its thread, and the turn it may be running.
use super::{Options, Program};
use crate::capability::SeatModel;
use crate::error::{Error, Result};
use crate::instance::{Launcher, Live};
use crate::turn::{self, Active, Turn};
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use tinyhivemind_hives::{AgentRunner, TurnDisposition, TurnFuture, TurnOutcome, TurnRequest};
use tinyhivemind_host::harness::{
    self, CancelFuture, CancellableRunner, Event, SeatPlan, ToolEndpoint,
};
use tokio::sync::{Mutex as AsyncMutex, watch};

/// A seat and its instance, if one is running.
#[derive(Debug)]
pub(super) struct Seat {
    program: Program,
    options: Options,
    plan: SeatPlan,
    tools: ToolEndpoint,
    model: SeatModel,
    live: AsyncMutex<Option<Arc<Live>>>,
    active: Mutex<Option<Active>>,
    closed: watch::Sender<bool>,
    turns: AtomicU64,
}

fn current(active: &Mutex<Option<Active>>) -> Option<Active> {
    active
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

fn clear(active: &Mutex<Option<Active>>) {
    *active.lock().unwrap_or_else(PoisonError::into_inner) = None;
}

impl Seat {
    pub(super) fn new(
        program: Program,
        options: Options,
        plan: SeatPlan,
        tools: ToolEndpoint,
        model: SeatModel,
    ) -> Self {
        Self {
            program,
            options,
            plan,
            tools,
            model,
            live: AsyncMutex::new(None),
            active: Mutex::new(None),
            closed: watch::channel(false).0,
            turns: AtomicU64::new(0),
        }
    }

    /// The thread a seat's first turn starts, and keeps for its life.
    fn thread(&self) -> String {
        format!("thm-{}", self.plan.seat.id)
    }

    /// Start an instance if none is running, reporting it.
    pub(super) async fn start(&self) -> Result<Arc<Live>> {
        let mut live = self.live.lock().await;
        if let Some(running) = live.as_ref().filter(|live| live.instance.exit().is_none()) {
            return Ok(running.clone());
        }
        if let Some(dead) = live.take() {
            dead.instance.stop(async {}, self.options.stop).await?;
        }
        let launcher = Launcher {
            program: &self.program,
            options: &self.options,
            plan: &self.plan,
            tools: &self.tools,
            model: &self.model,
        };
        let started = Arc::new(launcher.launch().await?);
        let event = Event::Started(started.started.clone());
        self.plan.observer.event(&self.plan.seat.id, event);
        *live = Some(started.clone());
        Ok(started)
    }

    async fn run(&self, request: TurnRequest) -> Result<TurnOutcome> {
        if *self.closed.borrow() {
            return Err(turn::Error::Closed.into());
        }
        let live = self.start().await?;
        let thread = request.session_id.clone().unwrap_or_else(|| self.thread());
        if let Some(session) = &request.session_id {
            let found = live
                .client
                .call(
                    "openhuman.threads_transcript_get",
                    json!({"thread_id": session}),
                )
                .await?;
            if found["data"]["hasTranscript"] != true {
                return Err(Error::Resume(session.clone()));
            }
        }
        let message = tinyhivemind_host::turn::render(&request)
            .map_err(|error| Error::Render(error.to_string()))?;
        let number = self.turns.fetch_add(1, Ordering::Relaxed);
        let turn = Turn {
            client: &live.client,
            seat: &self.plan.seat.id,
            thread: &thread,
            id: &request.turn_id,
            client_id: format!("thm-turn-{number}"),
            iterations: self.model.iterations,
            observer: self.plan.observer.as_ref(),
            approver: self.plan.approver.as_ref(),
            closed: self.closed.subscribe(),
        };
        let ended = turn::run(turn, &message, &self.active).await?;
        if let Some(usage) = ended.usage {
            let event = Event::Usage {
                turn_id: request.turn_id.clone(),
                usage,
            };
            self.plan.observer.event(&self.plan.seat.id, event);
        }
        let disposition = ended
            .failure
            .map_or(TurnDisposition::Completed, TurnDisposition::Failed);
        Ok(TurnOutcome {
            session_id: thread,
            reply: ended.reply,
            disposition,
        })
    }

    /// Cancel the submitted turn and show it stopped, or kill the instance.
    async fn cancel(&self) -> Result<()> {
        let Some(active) = current(&self.active) else {
            return Ok(());
        };
        let mut live = self.live.lock().await;
        let Some(running) = live.clone() else {
            // Stopped since, so the turn is over.
            clear(&self.active);
            return Ok(());
        };
        let alive = running.instance.exit().is_none();
        if alive && turn::cancel(&running.client, &active).await.is_ok() {
            clear(&self.active);
            return Ok(());
        }
        // Dead, or it could not show the turn stopped: then nothing of it may
        // run on.
        running.instance.stop(async {}, self.options.stop).await?;
        *live = None;
        clear(&self.active);
        Ok(())
    }

    /// Cancel, close the turn's events, then stop the instance and prove it.
    pub(super) async fn stop(&self) -> Result<()> {
        self.closed.send_replace(true);
        let Some(live) = self.live.lock().await.take() else {
            return Ok(());
        };
        let active = current(&self.active);
        let cancel = async {
            if let Some(active) = &active {
                // Best effort: the stop that follows ends the turn anyway.
                let _ = turn::cancel(&live.client, active).await;
            }
        };
        live.instance.stop(cancel, self.options.stop).await?;
        clear(&self.active);
        Ok(())
    }
}

/// The seat as the seat runner sees it.
pub(super) struct Runner(pub(super) Arc<Seat>);

impl AgentRunner for Runner {
    fn run(&self, request: TurnRequest) -> TurnFuture {
        let seat = self.0.clone();
        Box::pin(async move { Ok(seat.run(request).await.map_err(harness::Error::from)?) })
    }
}

impl CancellableRunner for Runner {
    fn cancel(&self, _: &str) -> CancelFuture {
        let seat = self.0.clone();
        Box::pin(async move { Ok(seat.cancel().await?) })
    }
}
