//! `live_*`: this runner on the pinned `openhuman-core`.
//!
//! Opt in with `THM_OPENHUMAN_CORE=<path to the pinned binary>`, fetched and
//! checked by `.github/scripts/fetch-harness.sh`; Node runs the spike
//! fixture, from `THM_NODE` or `PATH`. Without either, each test says why on
//! stderr and passes. The conformance cases run with the scripted model in
//! `live/model.rs` and the fixture's MCP stub; the gate repeats the spike's
//! checks with the fixture as model too.
#![cfg(target_os = "linux")]

#[path = "live/model.rs"]
mod model;
#[path = "support/seat.rs"]
mod seat_support;
#[path = "live/subject.rs"]
mod subject;

use anyhow::{Context as _, Result};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use subject::{Fixture, LiveSubject, PINNED, options, requirements};
use tinyhivemind_hives::{Destination, Message, TurnDisposition, TurnRequest};
use tinyhivemind_host::harness::conformance::Case;
use tinyhivemind_host::harness::{Event, Harness as _, Observer, SeatPlan, ToolEndpoint};
use tinyhivemind_openhuman_process::{OpenHumanHarness, Program};

macro_rules! live {
    ($($name:ident => $case:ident),* $(,)?) => {
        $(
            #[tokio::test(flavor = "multi_thread")]
            async fn $name() -> Result<()> {
                if let Some(subject) = LiveSubject::new().await? {
                    Case::$case.run(&subject).await?;
                }
                Ok(())
            }
        )*
        #[test]
        fn every_case_is_run_live() {
            assert_eq!(Case::ALL, [$(Case::$case),*]);
        }
    };
}

live! {
    live_a_second_turn_continues_the_session => SecondTurnContinues,
    live_a_session_is_resumed_after_a_restart => ResumesAfterRestart,
    live_a_resume_failure_is_an_error => ResumeFailureIsAnError,
    live_a_timeout_stops_the_turn => TimeoutStopsTheTurn,
    live_a_dropped_run_stops_the_turn => DropStopsTheTurn,
    live_a_kill_neither_replays_nor_loses_the_seat => KillNeitherReplaysNorLosesTheSeat,
    live_one_turn_runs_at_a_time => OneTurnAtATime,
    live_no_turn_runs_twice => NoTurnTwice,
    live_usage_is_per_turn => UsageIsPerTurn,
    live_the_tool_list_is_exact => ToolListIsExact,
    live_the_config_is_the_one_written => ConfigIsTheOneWritten,
    live_nothing_is_left_after_stop => NothingLeftAfterStop,
}

#[derive(Default)]
struct Recorder(Mutex<Vec<Event>>);

impl Observer for Recorder {
    fn event(&self, _: &str, event: Event) {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(event);
    }
}

fn request(turn: &str, body: &str, session: Option<&str>) -> TurnRequest {
    TurnRequest {
        turn_id: turn.into(),
        scheduled_job_id: None,
        teammates: Vec::new(),
        agent_id: "scout".into(),
        session_id: session.map(Into::into),
        messages: vec![Message {
            scheduled_job_id: None,
            message_id: format!("message-{turn}"),
            sequence: 1,
            sender: tinyhivemind_hives::HOST_ID.into(),
            destination: Destination::Agent("scout".into()),
            body: body.into(),
            thread: None,
            episode_id: None,
            only_for: Vec::new(),
        }],
        memberships: Vec::new(),
        episode: None,
        resumption: None,
    }
}

/// The spike's gate through this runner: the hive tool is called as the
/// right seat, the model is offered that tool alone, usage is the turn's,
/// a dropped turn closes its model call, and nothing is left after stop.
#[tokio::test(flavor = "multi_thread")]
async fn live_the_spike_gate_holds_through_the_runner() -> Result<()> {
    let (binary, node) = match requirements() {
        Ok(found) => found,
        Err(reason) => {
            eprintln!("skipped: {reason}");
            return Ok(());
        }
    };
    let root = tempfile::tempdir()?;
    let fixture = Fixture::start(&node, root.path()).await?;
    let harness = OpenHumanHarness::new(Program::new(binary, PINNED)).with_options(options());
    let endpoint = format!("http://127.0.0.1:{}/v1", fixture.port);
    let events = Arc::new(Recorder::default());
    let mut plan = SeatPlan::new(seat_support::seat(&endpoint), root.path().join("instance"));
    plan.observer = events.clone();
    let url = format!("http://127.0.0.1:{}/mcp", fixture.port);
    let tools = ToolEndpoint::new("hivea", url, "seat-a-token", ["hivemind_post".to_owned()]);
    let runner = harness.start(plan, tools).await?;

    let since = fixture.entries().len();
    let outcome = runner.run(request("t1", "CALLTOOL please", None)).await?;
    assert_eq!(outcome.disposition, TurnDisposition::Completed);
    assert_eq!(outcome.reply.as_deref(), Some("MOCK_REPLY turns=1"));
    let entries: Vec<Value> = fixture.entries().split_off(since);
    let calls: Vec<&Value> = entries
        .iter()
        .filter(|entry| entry["rpc"] == "tools/call")
        .collect();
    assert_eq!(calls.len(), 1, "{entries:?}");
    assert_eq!(calls[0]["seat"], "a");
    let models: Vec<&Value> = entries
        .iter()
        .filter(|entry| entry["kind"] == "model")
        .collect();
    let started = events
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .find_map(|event| match event {
            Event::Started(started) => Some(started.clone()),
            _ => None,
        });
    let started = started.context("no started event")?;
    assert_eq!(started.version, PINNED);
    let offered: Vec<Value> = started.tools.iter().map(|tool| json!(tool)).collect();
    assert_eq!(offered.len(), 1);
    for model in &models {
        assert_eq!(model["tools"], json!(offered), "the model saw other tools");
    }
    let usage = events
        .0
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .find_map(|event| match event {
            Event::Usage { turn_id, usage } if turn_id == "t1" => Some(*usage),
            _ => None,
        });
    assert_eq!(usage.map(|usage| usage.input_tokens), Some(200));

    let session = outcome.session_id;
    let slow = tokio::spawn(runner.run(request("t2", "SLOW please", Some(&session))));
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while !fixture.entries().iter().any(|entry| entry["key"] == "SLOW") {
        anyhow::ensure!(
            tokio::time::Instant::now() < deadline,
            "the slow turn never reached the model"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    slow.abort();
    while !fixture
        .entries()
        .iter()
        .any(|entry| entry["kind"] == "model-disconnect")
    {
        anyhow::ensure!(
            tokio::time::Instant::now() < deadline,
            "the dropped turn kept its model call open"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let again = runner
        .run(request("t3", "PLAIN again", Some(&session)))
        .await?;
    // The same thread, without the cancelled turn's message: the binary keeps
    // only turns that ended, as the spike saw too.
    assert_eq!(again.reply.as_deref(), Some("MOCK_REPLY turns=2"));
    assert_eq!(again.session_id, session);

    harness.stop("scout").await?;
    let left = seat_support::processes_in(&root.path().join("instance"));
    assert_eq!(left, Vec::<u32>::new());
    Ok(())
}
