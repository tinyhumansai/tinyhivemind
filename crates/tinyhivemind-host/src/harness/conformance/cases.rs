//! The cases themselves.
use super::{CallState, ModelStep, Subject, ToolFault};
use crate::harness::{Event, Harness, Observer, Started, TurnUsage};
use std::fmt::Display;
use std::future::Future;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tinyhivemind_hives::{
    AgentRunner, Destination, Message, TurnDisposition, TurnOutcome, TurnRequest,
};
use tokio::time::Instant;

type Outcome<T = ()> = Result<T, String>;

/// The limits the timeout case tries. A longer one is tried if a turn did not
/// reach the model, or an answered one did not end, so that a slow machine is
/// not mistaken for a wrong client.
const LIMITS: [Duration; 3] = [
    Duration::from_secs(2),
    Duration::from_secs(8),
    Duration::from_secs(25),
];
/// How long a turn that must not start is given to start anyway.
const QUIET: Duration = Duration::from_millis(200);
const POLL: Duration = Duration::from_millis(5);

fn ensure(holds: bool, otherwise: impl FnOnce() -> String) -> Outcome {
    if holds { Ok(()) } else { Err(otherwise()) }
}

fn text(error: impl Display) -> String {
    error.to_string()
}

/// Keeps every event.
#[derive(Default)]
struct Recorder(Mutex<Vec<Event>>);

impl Observer for Recorder {
    fn event(&self, _: &str, event: Event) {
        let mut events = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        events.push(event);
    }
}

impl Recorder {
    fn all(&self) -> Vec<Event> {
        let events = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        events.clone()
    }
    /// The instance last started.
    fn started(&self) -> Outcome<Started> {
        let started = self.all().into_iter().rev().find_map(|event| match event {
            Event::Started(started) => Some(started),
            _ => None,
        });
        started.ok_or_else(|| "no started event was reported".into())
    }
    /// The input tokens reported for a turn.
    fn usage(&self, turn: &str) -> Vec<u64> {
        let events = self.all().into_iter();
        let reported = events.filter_map(|event| match event {
            Event::Usage { turn_id, usage } if turn_id == turn => Some(usage.input_tokens),
            _ => None,
        });
        reported.collect()
    }
}

/// A started seat.
struct Seat<'a> {
    subject: &'a dyn Subject,
    harness: Arc<dyn Harness>,
    id: String,
    runner: Arc<dyn AgentRunner>,
    events: Arc<Recorder>,
}

async fn start(subject: &dyn Subject) -> Outcome<Seat<'_>> {
    start_limited(subject, None).await
}

async fn start_limited(subject: &dyn Subject, limit: Option<Duration>) -> Outcome<Seat<'_>> {
    let harness = subject.harness();
    let events = Arc::new(Recorder::default());
    let mut plan = subject.seat();
    plan.observer = events.clone();
    plan.seat.integration.limits.timeout_ms =
        limit.map(|limit| u64::try_from(limit.as_millis()).unwrap_or(u64::MAX));
    let id = plan.seat.id.clone();
    let starting = harness.start(plan, subject.endpoint());
    let runner = within(subject, "the start", starting).await?;
    Ok(Seat {
        subject,
        harness,
        id,
        runner: runner.map_err(text)?,
        events,
    })
}

impl Seat<'_> {
    /// Run a turn and require that it completes.
    async fn complete(&self, turn: &str, session: Option<&str>) -> Outcome<TurnOutcome> {
        let run = self.run(turn, session).await?;
        let outcome = run.map_err(|error| format!("turn {turn} failed: {error}"))?;
        ensure(outcome.disposition == TurnDisposition::Completed, || {
            format!("turn {turn} ended as {:?}", outcome.disposition)
        })?;
        ensure(!outcome.session_id.trim().is_empty(), || {
            format!("turn {turn} returned no session")
        })?;
        Ok(outcome)
    }

    /// Run a turn, however it ends.
    async fn run(
        &self,
        turn: &str,
        session: Option<&str>,
    ) -> Outcome<tinyhivemind_hives::Result<TurnOutcome>> {
        let run = self.runner.run(request(&self.id, turn, session));
        within(self.subject, &format!("turn {turn}"), run).await
    }

    /// Run a turn and require that it fails.
    async fn fail(&self, turn: &str, session: Option<&str>) -> Outcome {
        failed(turn, self.run(turn, session).await?)
    }

    async fn stop(&self) -> Outcome {
        let stopping = self.harness.stop(&self.id);
        let stopped = within(self.subject, "the stop", stopping).await?;
        stopped.map_err(|error| format!("the stop failed: {error}"))
    }

    /// One more turn in `session`, which must see `seen` user messages or more.
    async fn goes_on(&self, turn: &str, session: &str, seen: usize) -> Outcome {
        self.subject.script(&[reply("on", 1)]);
        let outcome = self.complete(turn, Some(session)).await?;
        ensure(outcome.session_id == session, || {
            format!("turn {turn} ran in another session")
        })?;
        let found = self.subject.model_calls().last().map(|call| call.seen);
        ensure(found >= Some(seen), || {
            format!("turn {turn} saw {found:?} user messages, not {seen} or more")
        })
    }
}

/// Whichever way a runner says so.
fn failed(turn: &str, ended: tinyhivemind_hives::Result<TurnOutcome>) -> Outcome {
    match ended {
        Err(_) => Ok(()),
        Ok(outcome) => ensure(
            matches!(outcome.disposition, TurnDisposition::Failed(_)),
            || format!("turn {turn} ended as {:?}", outcome.disposition),
        ),
    }
}

fn reply(text: &str, tokens: u64) -> ModelStep {
    ModelStep::Reply {
        text: text.into(),
        usage: TurnUsage {
            input_tokens: tokens,
            ..TurnUsage::default()
        },
    }
}

fn request(seat: &str, turn: &str, session: Option<&str>) -> TurnRequest {
    TurnRequest {
        turn_id: turn.into(),
        scheduled_job_id: None,
        teammates: Vec::new(),
        agent_id: seat.into(),
        session_id: session.map(Into::into),
        messages: vec![Message {
            scheduled_job_id: None,
            message_id: format!("message-{turn}"),
            sequence: 1,
            sender: tinyhivemind_hives::HOST_ID.into(),
            destination: Destination::Agent(seat.into()),
            body: format!("conformance {turn}"),
            thread: None,
            episode_id: None,
            only_for: Vec::new(),
        }],
        memberships: Vec::new(),
        episode: None,
        resumption: None,
    }
}

async fn within<T>(
    subject: &dyn Subject,
    what: &str,
    future: impl Future<Output = T>,
) -> Outcome<T> {
    let patience = subject.patience();
    let ended = tokio::time::timeout(patience, future).await;
    ended.map_err(|_| format!("{what} did not end within {patience:?}"))
}

/// Wait until call number `index` exists and is in `state`.
async fn call_is(subject: &dyn Subject, index: usize, state: CallState) -> Outcome {
    let deadline = Instant::now() + subject.patience();
    loop {
        let found = subject.model_calls().get(index).map(|call| call.state);
        if found == Some(state) {
            return Ok(());
        }
        ensure(Instant::now() < deadline, || {
            format!("model call {index} is {found:?}, not {state:?}")
        })?;
        tokio::time::sleep(POLL).await;
    }
}

fn calls_are(subject: &dyn Subject, expected: &[CallState]) -> Outcome {
    let calls = subject.model_calls();
    let states: Vec<_> = calls.iter().map(|call| call.state).collect();
    ensure(states == expected, || {
        format!("the model calls are {states:?}, not {expected:?}")
    })
}

fn nothing_runs(subject: &dyn Subject) -> Outcome {
    let left = subject.processes();
    ensure(left.is_empty(), || format!("processes {left:?} are left"))
}

pub(super) async fn second_turn_continues(subject: &dyn Subject) -> Outcome {
    let seat = start(subject).await?;
    subject.script(&[reply("one", 1)]);
    let first = seat.complete("t1", None).await?;
    ensure(first.reply.as_deref() == Some("one"), || {
        format!("the reply is {:?}", first.reply)
    })?;
    seat.goes_on("t2", &first.session_id, 2).await?;
    seat.stop().await
}

pub(super) async fn resumes_after_restart(subject: &dyn Subject) -> Outcome {
    let seat = start(subject).await?;
    subject.script(&[reply("one", 1)]);
    let session = seat.complete("t1", None).await?.session_id;
    seat.stop().await?;
    drop(seat);
    let seat = start(subject).await?;
    seat.goes_on("t2", &session, 2).await?;
    seat.stop().await
}

pub(super) async fn resume_failure_is_an_error(subject: &dyn Subject) -> Outcome {
    let seat = start(subject).await?;
    subject.script(&[reply("one", 1)]);
    let session = seat.complete("t1", None).await?.session_id;
    seat.stop().await?;
    drop(seat);
    subject.lose_sessions();
    subject.script(&[reply("two", 1)]);
    let seat = start(subject).await?;
    seat.fail("t2", Some(&session)).await?;
    // Nothing was asked of the model in some other session.
    calls_are(subject, &[CallState::Answered])?;
    seat.stop().await
}

pub(super) async fn timeout_stops_the_turn(subject: &dyn Subject) -> Outcome {
    let mut reason = String::new();
    for limit in LIMITS {
        let seat = start_limited(subject, Some(limit)).await?;
        subject.script(&[ModelStep::Hang]);
        let call = subject.model_calls().len();
        seat.fail("t1", None).await?;
        if subject.model_calls().len() > call {
            call_is(subject, call, CallState::Aborted).await?;
            match goes_on_within_its_limit(&seat).await {
                Ok(()) => return seat.stop().await,
                // Or too slow, which a longer limit tells apart.
                Err(error) => reason = error,
            }
        } else {
            reason = "no turn reached the model within its limit".into();
        }
        seat.stop().await?;
    }
    Err(reason)
}

async fn goes_on_within_its_limit(seat: &Seat<'_>) -> Outcome {
    seat.subject.script(&[reply("two", 1)]);
    let session = seat.complete("t2", None).await?.session_id;
    seat.goes_on("t3", &session, 2).await
}

pub(super) async fn drop_stops_the_turn(subject: &dyn Subject) -> Outcome {
    let seat = start(subject).await?;
    subject.script(&[reply("one", 1)]);
    let session = seat.complete("t1", None).await?.session_id;
    subject.script(&[ModelStep::Hang]);
    let run = tokio::spawn(seat.runner.run(request(&seat.id, "t2", Some(&session))));
    call_is(subject, 1, CallState::Open).await?;
    run.abort();
    call_is(subject, 1, CallState::Aborted).await?;
    seat.goes_on("t3", &session, 2).await?;
    seat.stop().await
}

pub(super) async fn kill_mid_turn(subject: &dyn Subject) -> Outcome {
    let seat = start(subject).await?;
    subject.script(&[reply("one", 1)]);
    let session = seat.complete("t1", None).await?.session_id;
    subject.script(&[ModelStep::Hang]);
    let run = tokio::spawn(seat.runner.run(request(&seat.id, "t2", Some(&session))));
    call_is(subject, 1, CallState::Open).await?;
    let pid = i32::try_from(seat.events.started()?.pid).map_err(text)?;
    let pid = rustix::process::Pid::from_raw(pid).ok_or("the instance has no pid")?;
    rustix::process::kill_process(pid, rustix::process::Signal::KILL).map_err(text)?;
    let ended = within(subject, "the killed turn", run).await?;
    failed("t2", ended.map_err(text)?)?;
    seat.goes_on("t3", &session, 2).await?;
    // Asked for again, it stays failed.
    seat.fail("t2", Some(&session)).await?;
    let expected = [CallState::Answered, CallState::Aborted, CallState::Answered];
    calls_are(subject, &expected)?;
    seat.stop().await
}

pub(super) async fn one_turn_at_a_time(subject: &dyn Subject) -> Outcome {
    let seat = start(subject).await?;
    subject.script(&[reply("one", 1)]);
    let session = seat.complete("t1", None).await?.session_id;
    subject.script(&[ModelStep::Hang, reply("three", 1)]);
    let first = tokio::spawn(seat.runner.run(request(&seat.id, "t2", Some(&session))));
    call_is(subject, 1, CallState::Open).await?;
    let second = tokio::spawn(seat.runner.run(request(&seat.id, "t3", Some(&session))));
    tokio::time::sleep(QUIET).await;
    calls_are(subject, &[CallState::Answered, CallState::Open])?;
    first.abort();
    let second = within(subject, "the waiting turn", second).await?;
    second.map_err(text)?.map_err(text)?;
    let expected = [CallState::Answered, CallState::Aborted, CallState::Answered];
    calls_are(subject, &expected)?;
    seat.stop().await
}

pub(super) async fn no_turn_twice(subject: &dyn Subject) -> Outcome {
    let seat = start(subject).await?;
    subject.script(&[reply("one", 1)]);
    let session = seat.complete("t1", None).await?.session_id;
    subject.script(&[reply("two", 1)]);
    seat.fail("t1", Some(&session)).await?;
    calls_are(subject, &[CallState::Answered])?;
    seat.stop().await
}

pub(super) async fn usage_is_per_turn(subject: &dyn Subject) -> Outcome {
    let seat = start(subject).await?;
    subject.script(&[reply("one", 100)]);
    let session = seat.complete("t1", None).await?.session_id;
    subject.script(&[reply("two", 30)]);
    seat.complete("t2", Some(&session)).await?;
    let found = (seat.events.usage("t1"), seat.events.usage("t2"));
    ensure(found == (vec![100], vec![30]), || {
        format!("turns that used 100 and 30 tokens reported {found:?}")
    })?;
    seat.stop().await?;
    drop(seat);
    let seat = start(subject).await?;
    subject.script(&[reply("three", 7)]);
    seat.complete("t3", Some(&session)).await?;
    let found = seat.events.usage("t3");
    ensure(found == [7], || {
        format!("a turn that used 7 tokens after a restart reported {found:?}")
    })?;
    seat.stop().await
}

pub(super) async fn tool_list_is_exact(subject: &dyn Subject) -> Outcome {
    for fault in [ToolFault::Extra, ToolFault::Missing] {
        subject.offer(fault);
        match start(subject).await {
            Ok(seat) => {
                seat.stop().await?;
                return Err(format!("a seat started with a tool list that is {fault:?}"));
            }
            Err(_) => nothing_runs(subject)?,
        }
    }
    subject.offer(ToolFault::None);
    let seat = start(subject).await?;
    let offered = seat.events.started()?.tools;
    // A harness may put the server's name around a tool's, but no less.
    let absent: Vec<_> = subject
        .endpoint()
        .tools
        .into_iter()
        .filter(|tool| !offered.iter().any(|name| name.contains(tool)))
        .collect();
    ensure(absent.is_empty(), || {
        format!("{absent:?} are not among the reported {offered:?}")
    })?;
    seat.stop().await
}

pub(super) async fn config_is_the_one_written(subject: &dyn Subject) -> Outcome {
    let seat = start(subject).await?;
    let compared = seat.events.started()?.settings;
    ensure(!compared.is_empty(), || "no setting was compared".into())?;
    seat.stop().await?;
    drop(seat);
    subject.misconfigure();
    if let Ok(seat) = start(subject).await {
        seat.stop().await?;
        return Err("a seat started with another config than was written".into());
    }
    nothing_runs(subject)
}

pub(super) async fn nothing_left_after_stop(subject: &dyn Subject) -> Outcome {
    let seat = start(subject).await?;
    subject.script(&[reply("one", 1)]);
    seat.complete("t1", None).await?;
    let escapee = within(subject, "the escape", subject.escapee()).await?;
    let running = subject.processes();
    let pid = seat.events.started()?.pid;
    for expected in [Some(pid), escapee].into_iter().flatten() {
        ensure(running.contains(&expected), || {
            format!("{expected} is not among the running {running:?}")
        })?;
    }
    seat.stop().await?;
    nothing_runs(subject)
}
