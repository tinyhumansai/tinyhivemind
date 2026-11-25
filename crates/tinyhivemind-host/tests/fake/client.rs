//! A client for the fake child, over the same port a real one implements.
use rustix::process::{Pid, Signal, kill_process};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;
use tinyhivemind_hives::{AgentRunner, TurnDisposition, TurnFuture, TurnOutcome, TurnRequest};
use tinyhivemind_host::harness::{
    CancelFuture, CancellableRunner, Error, Event, Expected, Harness, HarnessKind, Result,
    SeatPlan, SeatRunner, StartFuture, StopFuture, ToolEndpoint, TurnUsage,
};
use tinyhivemind_host::process::{Instance, InstanceDir, JsonLines, Launch, StopPolicy};
use tokio::sync::Mutex as AsyncMutex;

const VERSION: &str = "1.0.0";
const READY: Duration = Duration::from_secs(30);
const STOP: StopPolicy = StopPolicy {
    cancel: Duration::from_secs(1),
    terminate: Duration::from_millis(200),
    kill: Duration::from_secs(30),
};

/// One thing this client, or its child, does wrong.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Fault {
    /// Nothing.
    None,
    /// The child starts a new session when it cannot resume.
    NewSession,
    /// The child resumes the session without its history.
    Forgetful,
    /// The child goes on with a cancelled turn.
    IgnoresCancel,
    /// Usage is the running total.
    TotalUsage,
    /// The instance is not verified.
    Unverified,
    /// Turns are not serialized, timed out or refused a second time.
    Unguarded,
    /// A turn interrupted by a crash is sent again.
    Replays,
    /// A crashed instance is not started again.
    StaysDown,
    /// Stopping kills the child alone.
    LeavesOrphans,
    /// A start that fails leaves its instance running.
    LeavesFailedStart,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What every seat of one fake harness shares.
#[derive(Clone)]
struct Shared {
    program: PathBuf,
    control: PathBuf,
    fault: Fault,
}

/// The fake harness.
pub(crate) struct FakeHarness {
    shared: Shared,
    seats: Arc<Mutex<BTreeMap<String, Arc<Seat>>>>,
}

impl FakeHarness {
    pub(crate) fn new(program: PathBuf, control: PathBuf, fault: Fault) -> Self {
        Self {
            shared: Shared {
                program,
                control,
                fault,
            },
            seats: Arc::default(),
        }
    }
}

impl Harness for FakeHarness {
    fn kind(&self) -> HarnessKind {
        HarnessKind::Codex
    }

    fn start(&self, plan: SeatPlan, tools: ToolEndpoint) -> StartFuture {
        let seats = self.seats.clone();
        let seat = Arc::new(Seat {
            shared: self.shared.clone(),
            plan,
            tools,
            live: AsyncMutex::new(None),
            total: Mutex::default(),
            pid: Mutex::default(),
        });
        Box::pin(async move {
            let live = seat.launch().await?;
            *seat.live.lock().await = Some(live);
            let id = seat.plan.seat.id.clone();
            let timeout = seat.plan.turn_timeout();
            let inner = Arc::new(Runner(seat.clone()));
            let unguarded = seat.shared.fault == Fault::Unguarded;
            lock(&seats).insert(id, seat);
            if unguarded {
                return Ok(inner as Arc<dyn AgentRunner>);
            }
            let runner = SeatRunner::new(inner).with_timeout(timeout);
            Ok(Arc::new(runner) as Arc<dyn AgentRunner>)
        })
    }

    fn stop(&self, seat_id: &str) -> StopFuture {
        let seat = lock(&self.seats).remove(seat_id);
        Box::pin(async move {
            if let Some(seat) = seat {
                seat.stop().await?;
            }
            Ok(())
        })
    }
}

/// A running, verified child and its streams.
struct Live {
    instance: Instance,
    lines: JsonLines,
}

impl Live {
    async fn send(&mut self, value: &Value) -> Result<()> {
        let sent = self.lines.sender.send(value).await;
        sent.map_err(|error| Error::Harness(error.to_string()))
    }
    async fn recv(&mut self) -> Result<Value> {
        match self.lines.receiver.recv().await {
            Some(Ok(value)) => Ok(value),
            Some(Err(error)) => Err(Error::Harness(error.to_string())),
            None => Err(Error::Harness("the child closed its output".into())),
        }
    }
}

struct Seat {
    shared: Shared,
    plan: SeatPlan,
    tools: ToolEndpoint,
    live: AsyncMutex<Option<Live>>,
    total: Mutex<u64>,
    pid: Mutex<u32>,
}

impl Seat {
    /// Start and verify a child, or leave nothing running.
    async fn launch(&self) -> Result<Live> {
        let dir = InstanceDir::create(&self.plan.directory)?;
        let fault = match self.shared.fault {
            Fault::NewSession => "new_session",
            Fault::Forgetful => "forgetful",
            Fault::IgnoresCancel => "ignores_cancel",
            _ => "",
        };
        let launch = Launch::new(&self.shared.program, dir)
            .env("CONTROL", &self.shared.control)
            .env("FAULT", fault)
            .json_lines();
        let mut instance = Instance::start(launch).await?;
        let lines = instance
            .take_lines()
            .ok_or_else(|| Error::Harness("no lines".into()))?;
        let mut live = Live { instance, lines };
        *lock(&self.total) = 0;
        *lock(&self.pid) = live.instance.pid();
        match self.verify(&mut live).await {
            Ok(()) => Ok(live),
            Err(error) if self.shared.fault == Fault::LeavesFailedStart => {
                std::mem::forget(live);
                Err(error)
            }
            Err(error) => {
                live.instance.stop(async {}, STOP).await?;
                Err(error)
            }
        }
    }

    async fn verify(&self, live: &mut Live) -> Result<()> {
        let tools = &self.tools.tools;
        live.send(&json!({"op": "hello", "tools": tools})).await?;
        let Live { instance, lines } = live;
        let hello = instance
            .ready(
                async {
                    let hello = lines.receiver.recv().await;
                    hello.ok_or("no answer")?.map_err(|_| "a bad answer")
                },
                READY,
            )
            .await?;
        let pid = hello["pid"]
            .as_u64()
            .and_then(|pid| u32::try_from(pid).ok());
        let found = hello["tools"].as_array().cloned().unwrap_or_default();
        let found: Vec<String> = found
            .iter()
            .filter_map(|tool| tool.as_str().map(Into::into))
            .collect();
        let version = hello["version"].as_str().unwrap_or_default();
        let required: Vec<String> = match self.shared.fault {
            Fault::Unverified => found.clone(),
            _ => tools.iter().cloned().collect(),
        };
        let started = Expected::new(instance.pid(), VERSION, required).verify(
            pid.unwrap_or_default(),
            version,
            found,
        )?;
        let id = &self.plan.seat.id;
        self.plan.observer.event(id, Event::Started(started));
        Ok(())
    }

    /// The live child, started again if it died.
    async fn revive<'a>(&self, live: &'a mut Option<Live>) -> Result<&'a mut Live> {
        let dead = live
            .as_ref()
            .is_none_or(|live| live.instance.exit().is_some());
        if dead && self.shared.fault != Fault::StaysDown {
            if let Some(old) = live.take() {
                old.instance.stop(async {}, STOP).await?;
            }
            *live = Some(self.launch().await?);
        }
        live.as_mut().ok_or_else(|| Error::Harness("down".into()))
    }

    async fn run(&self, request: TurnRequest) -> Result<TurnOutcome> {
        let mut live = self.live.lock().await;
        let ended = self.turn(&mut live, &request).await;
        if ended.is_err()
            && self.shared.fault == Fault::Replays
            && live
                .as_ref()
                .is_some_and(|live| live.instance.exit().is_some())
        {
            return self.turn(&mut live, &request).await;
        }
        ended
    }

    async fn turn(&self, live: &mut Option<Live>, request: &TurnRequest) -> Result<TurnOutcome> {
        let live = self.revive(live).await?;
        live.send(&json!({
            "op": "turn", "turn": request.turn_id, "session": request.session_id,
        }))
        .await?;
        let answer = loop {
            let answer = match live.recv().await {
                Ok(answer) => answer,
                Err(error) => {
                    // The output ends before the exit is observed.
                    live.instance.exited().await;
                    return Err(error);
                }
            };
            // The answer to a cancellation of an earlier turn.
            if answer["event"] != "cancelled" {
                break answer;
            }
        };
        if answer["event"] == "no_session" {
            return Err(Error::Resume {
                session_id: request.session_id.clone().unwrap_or_default(),
                reason: answer["reason"].to_string(),
            });
        }
        if answer["event"] != "done" {
            return Err(Error::Harness(answer.to_string()));
        }
        let total = answer["total"].as_u64().unwrap_or_default();
        let usage = |tokens| TurnUsage {
            input_tokens: tokens,
            ..TurnUsage::default()
        };
        let earlier = std::mem::replace(&mut *lock(&self.total), total);
        let earlier = if self.shared.fault == Fault::TotalUsage {
            0
        } else {
            earlier
        };
        let used = usage(total)
            .since(usage(earlier))
            .ok_or_else(|| Error::Harness("usage went down".into()))?;
        let event = Event::Usage {
            turn_id: request.turn_id.clone(),
            usage: used,
        };
        self.plan.observer.event(&self.plan.seat.id, event);
        Ok(TurnOutcome {
            session_id: answer["session"].as_str().unwrap_or_default().into(),
            reply: answer["reply"].as_str().map(Into::into),
            disposition: TurnDisposition::Completed,
        })
    }

    /// Cancel and wait for the answer. A dead child has nothing to cancel.
    async fn cancel(&self) -> Result<()> {
        let mut live = self.live.lock().await;
        let Some(live) = live.as_mut() else {
            return Ok(());
        };
        if live.send(&json!({"op": "cancel"})).await.is_err() {
            return Ok(());
        }
        loop {
            match live.recv().await {
                Ok(answer) if answer["event"] == "cancelled" => return Ok(()),
                // The turn ended first. What it used is not the next one's.
                Ok(answer) => {
                    if let Some(total) = answer["total"].as_u64() {
                        *lock(&self.total) = total;
                    }
                }
                Err(_) => {
                    live.instance.exited().await;
                    return Ok(());
                }
            }
        }
    }

    async fn stop(&self) -> Result<()> {
        if self.shared.fault == Fault::LeavesOrphans {
            if let Some(pid) = Pid::from_raw(lock(&self.pid).cast_signed()) {
                let _ = kill_process(pid, Signal::KILL);
            }
            std::mem::forget(self.live.lock().await.take());
            return Ok(());
        }
        if let Some(live) = self.live.lock().await.take() {
            let Live { instance, lines } = live;
            instance.stop(async { drop(lines) }, STOP).await?;
        }
        Ok(())
    }

    async fn escape(&self) -> Result<()> {
        if let Some(live) = self.live.lock().await.as_mut() {
            live.send(&json!({"op": "escape"})).await?;
        }
        Ok(())
    }
}

impl FakeHarness {
    /// Have every running seat leave a grandchild in a session of its own.
    pub(crate) async fn escape(&self) -> Result<()> {
        let seats: Vec<_> = lock(&self.seats).values().cloned().collect();
        for seat in seats {
            seat.escape().await?;
        }
        Ok(())
    }
}

struct Runner(Arc<Seat>);

impl AgentRunner for Runner {
    fn run(&self, request: TurnRequest) -> TurnFuture {
        let seat = self.0.clone();
        Box::pin(async move { Ok(seat.run(request).await?) })
    }
}

impl CancellableRunner for Runner {
    fn cancel(&self, _: &str) -> CancelFuture {
        let seat = self.0.clone();
        Box::pin(async move { seat.cancel().await })
    }
}
