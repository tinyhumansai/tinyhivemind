//! A scripted runner in this process, and plans to start seats with.
use crate::harness::{CancelFuture, CancellableRunner, Error, SeatPlan};
use serde_json::Value;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tinyhivemind_hives::{AgentRunner, TurnDisposition, TurnFuture, TurnOutcome, TurnRequest};
use tinyhivemind_lang::{LoweredSeat, LoweredSeatIntegration};
use tokio::sync::Semaphore;

pub(super) fn plan(id: &str, root: &Path) -> SeatPlan {
    let seat = LoweredSeat {
        id: id.into(),
        label: id.into(),
        template: "seat".into(),
        config: Value::Null,
        prompt: String::new(),
        memory: None,
        integration: LoweredSeatIntegration::default(),
    };
    SeatPlan::new(seat, root)
}

pub(super) fn request(turn_id: &str, session_id: Option<&str>) -> TurnRequest {
    TurnRequest {
        turn_id: turn_id.into(),
        scheduled_job_id: None,
        teammates: Vec::new(),
        agent_id: "scout".into(),
        session_id: session_id.map(Into::into),
        messages: Vec::new(),
        memberships: Vec::new(),
        episode: None,
        resumption: None,
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What a scripted turn does.
#[derive(Clone)]
pub(super) enum Turn {
    /// Return this session.
    Session(&'static str),
    /// Return the session `s`, this way.
    Ends(TurnDisposition),
    /// Fail.
    Fail,
    /// Never return.
    Hang,
}

/// A runner that keeps a log, plays a script, and whose cancellations wait for
/// a permit.
pub(super) struct Scripted {
    log: Mutex<Vec<String>>,
    turns: Mutex<Vec<Turn>>,
    pub(super) permits: Semaphore,
    refuse_cancel: Mutex<bool>,
}

impl Scripted {
    pub(super) fn new(turns: &[Turn]) -> Arc<Self> {
        Arc::new(Self {
            log: Mutex::default(),
            turns: Mutex::new(turns.iter().rev().cloned().collect()),
            permits: Semaphore::new(Semaphore::MAX_PERMITS),
            refuse_cancel: Mutex::default(),
        })
    }
    pub(super) fn log(&self) -> Vec<String> {
        lock(&self.log).clone()
    }
    /// From now on, refuse to cancel or stop refusing.
    pub(super) fn refuse_cancel(&self, refuse: bool) {
        *lock(&self.refuse_cancel) = refuse;
    }
    fn note(&self, entry: String) {
        lock(&self.log).push(entry);
    }
}

/// The trait methods take `&self`, so the runner is shared through a handle.
#[derive(Clone)]
pub(super) struct Handle(pub(super) Arc<Scripted>);

impl AgentRunner for Handle {
    fn run(&self, request: TurnRequest) -> TurnFuture {
        let this = self.0.clone();
        Box::pin(async move {
            let session = request.session_id.as_deref().unwrap_or("none");
            this.note(format!("run {} {session}", request.turn_id));
            let turn = lock(&this.turns).pop();
            match turn.ok_or(Error::Harness("the script is over".into()))? {
                Turn::Session(session_id) => Ok(TurnOutcome {
                    session_id: session_id.into(),
                    reply: None,
                    disposition: TurnDisposition::Completed,
                }),
                Turn::Ends(disposition) => Ok(TurnOutcome {
                    session_id: "s".into(),
                    reply: None,
                    disposition,
                }),
                Turn::Fail => Err(Error::Harness("scripted".into()).into()),
                Turn::Hang => std::future::pending().await,
            }
        })
    }
}

impl CancellableRunner for Handle {
    fn cancel(&self, turn_id: &str) -> CancelFuture {
        let this = self.0.clone();
        let turn_id = turn_id.to_owned();
        Box::pin(async move {
            this.note(format!("cancel {turn_id}"));
            if let Ok(permit) = this.permits.acquire().await {
                permit.forget();
            }
            if *lock(&this.refuse_cancel) {
                return Err(Error::Harness("will not stop".into()));
            }
            this.note(format!("cancelled {turn_id}"));
            Ok(())
        })
    }
}
