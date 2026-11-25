//! The conformance suite over the scripted fake harness, and proof that each
//! case fails a harness that breaks its rule.
//!
//! Needs the `conformance` feature, which builds the fake's child binary, and
//! Linux, because the fake escapes its process group on purpose.
#![cfg(target_os = "linux")]

#[path = "fake/client.rs"]
mod client;
#[path = "fake/subject.rs"]
mod subject;

use client::Fault;
use subject::FakeSubject;
use tinyhivemind_host::harness::conformance::Case;

/// One test per case, and a check that no case is left out.
macro_rules! passes {
    ($($name:ident => $case:ident),* $(,)?) => {
        $(
            #[tokio::test]
            async fn $name() {
                Case::$case.run(&FakeSubject::new(Fault::None)).await.unwrap();
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
    nothing_is_left_after_stop => NothingLeftAfterStop,
}

/// `case` must fail a harness with `fault`.
async fn catches(fault: Fault, case: Case) {
    let failure = case.run(&FakeSubject::new(fault)).await.unwrap_err();
    assert_eq!(failure.case, case);
    assert!(!failure.reason.is_empty());
    assert!(failure.to_string().contains(&failure.reason));
}

#[tokio::test]
async fn a_second_turn_without_the_first_is_caught() {
    catches(Fault::Forgetful, Case::SecondTurnContinues).await;
    catches(Fault::Forgetful, Case::ResumesAfterRestart).await;
}

#[tokio::test]
async fn a_new_session_in_place_of_a_lost_one_is_caught() {
    catches(Fault::NewSession, Case::ResumeFailureIsAnError).await;
}

#[tokio::test]
async fn a_cancellation_that_does_nothing_is_caught() {
    catches(Fault::IgnoresCancel, Case::DropStopsTheTurn).await;
    catches(Fault::IgnoresCancel, Case::TimeoutStopsTheTurn).await;
}

#[tokio::test]
async fn a_runner_without_the_seat_rules_is_caught() {
    catches(Fault::Unguarded, Case::TimeoutStopsTheTurn).await;
    catches(Fault::Unguarded, Case::DropStopsTheTurn).await;
    catches(Fault::Unguarded, Case::NoTurnTwice).await;
}

#[tokio::test]
async fn a_replayed_turn_is_caught() {
    catches(Fault::Replays, Case::KillNeitherReplaysNorLosesTheSeat).await;
}

#[tokio::test]
async fn a_lost_seat_is_caught() {
    catches(Fault::StaysDown, Case::KillNeitherReplaysNorLosesTheSeat).await;
}

#[tokio::test]
async fn cumulative_usage_is_caught() {
    catches(Fault::TotalUsage, Case::UsageIsPerTurn).await;
}

#[tokio::test]
async fn an_unverified_tool_list_is_caught() {
    catches(Fault::Unverified, Case::ToolListIsExact).await;
}

#[tokio::test]
async fn an_instance_left_by_a_failed_start_is_caught() {
    catches(Fault::LeavesFailedStart, Case::ToolListIsExact).await;
}

#[tokio::test]
async fn an_orphan_left_by_stop_is_caught() {
    catches(Fault::LeavesOrphans, Case::NothingLeftAfterStop).await;
}
