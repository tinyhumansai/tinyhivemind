//! The host's conformance suite over this runner and a fake `openhuman-core`
//! child, so that supervision is real; and the seat refusals through the
//! port. Linux only, as the suite escapes a process group on purpose.
#![cfg(target_os = "linux")]

#[path = "support/seat.rs"]
mod seat_support;
#[path = "fake/subject.rs"]
mod subject;

use anyhow::Result;
use subject::FakeSubject;
use tinyhivemind_hives::TurnDisposition;
use tinyhivemind_host::harness::conformance::{Case, Subject as _};
use tinyhivemind_host::harness::{Error, HarnessKind, SeatPlan};
use tinyhivemind_openhuman_process::capability::Refusal;

/// One test per case, and a check that no case is left out.
macro_rules! passes {
    ($($name:ident => $case:ident),* $(,)?) => {
        $(
            #[tokio::test(flavor = "multi_thread")]
            async fn $name() -> Result<()> {
                Case::$case.run(&FakeSubject::new()?).await?;
                Ok(())
            }
        )*
        #[test]
        fn every_case_is_run() {
            assert_eq!(Case::ALL, [$(Case::$case),*]);
        }
    };
}

passes! {
    a_second_turn_continues_the_session => SecondTurnContinues,
    a_session_is_resumed_after_a_restart => ResumesAfterRestart,
    a_resume_failure_is_an_error => ResumeFailureIsAnError,
    a_timeout_stops_the_turn => TimeoutStopsTheTurn,
    a_dropped_run_stops_the_turn => DropStopsTheTurn,
    a_kill_neither_replays_nor_loses_the_seat => KillNeitherReplaysNorLosesTheSeat,
    one_turn_runs_at_a_time => OneTurnAtATime,
    no_turn_runs_twice => NoTurnTwice,
    usage_is_per_turn => UsageIsPerTurn,
    the_tool_list_is_exact => ToolListIsExact,
    the_config_is_the_one_written => ConfigIsTheOneWritten,
    nothing_is_left_after_stop => NothingLeftAfterStop,
}

#[tokio::test]
async fn the_harness_is_the_openhuman_kind() -> Result<()> {
    let subject = FakeSubject::new()?;
    assert_eq!(subject.harness().kind(), HarnessKind::OpenHuman);
    Ok(())
}

#[tokio::test]
async fn a_seat_it_cannot_honour_is_refused_before_anything_starts() -> Result<()> {
    let subject = FakeSubject::new()?;
    let mut seat = seat_support::seat("http://127.0.0.1:9/v1");
    seat.integration.limits.iterations = Some(20);
    let plan = SeatPlan::new(seat, subject.root_path().join("instance"));
    let refused = subject.harness().start(plan, subject.endpoint()).await;
    let Err(Error::Refused(refusal)) = refused else {
        anyhow::bail!("not refused: {:?}", refused.err());
    };
    let refusal = refusal.downcast_ref::<Refusal>();
    assert_eq!(refusal, Some(&Refusal::Iterations { asked: 20, cap: 8 }));
    assert_eq!(subject.processes(), Vec::<u32>::new());
    Ok(())
}

#[tokio::test]
async fn a_running_seat_is_not_started_twice_and_a_stopped_one_is_already_stopped() -> Result<()> {
    let subject = FakeSubject::new()?;
    let harness = subject.harness();
    let runner = harness.start(subject.seat(), subject.endpoint()).await?;
    let again = harness.start(subject.seat(), subject.endpoint()).await;
    assert!(matches!(again, Err(Error::Harness(reason)) if reason.contains("already running")));
    harness.stop("scout").await?;
    harness.stop("scout").await?;
    assert_eq!(subject.processes(), Vec::<u32>::new());
    drop(runner);
    Ok(())
}

/// A first turn that fails leaves no session behind: the next turn starts
/// the seat's thread afresh instead of failing to resume it forever.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_first_turn_leaves_the_seat_able_to_start() -> Result<()> {
    let subject = FakeSubject::new()?;
    let harness = subject.harness();
    let runner = harness.start(subject.seat(), subject.endpoint()).await?;
    let steps = subject.control().join("steps");
    std::fs::write(steps.join("a"), r#"{"error": "provider down"}"#)?;
    let first = runner.run(seat_support::request("t1", None)).await;
    let failed = match &first {
        Ok(outcome) => matches!(outcome.disposition, TurnDisposition::Failed(_)),
        Err(_) => true,
    };
    assert!(failed, "{first:?}");
    std::fs::write(steps.join("b"), r#"{"reply": "works", "tokens": 1}"#)?;
    let second = runner.run(seat_support::request("t2", None)).await?;
    assert_eq!(second.disposition, TurnDisposition::Completed);
    assert_eq!(second.reply.as_deref(), Some("works"));
    harness.stop("scout").await?;
    Ok(())
}
