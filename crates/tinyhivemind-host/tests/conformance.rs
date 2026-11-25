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

use anyhow::{Context as _, Result};
use client::Fault;
use subject::FakeSubject;
use tinyhivemind_host::harness::conformance::Case;

/// One test per case, and a check that no case is left out.
macro_rules! passes {
    ($($name:ident => $case:ident),* $(,)?) => {
        $(
            #[tokio::test]
            async fn $name() -> Result<()> {
                Case::$case.run(&FakeSubject::new(Fault::None)?).await?;
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

/// `case` must fail a harness with `fault`, and for that reason.
async fn catches(fault: Fault, case: Case, reason: &str) -> Result<()> {
    let subject = FakeSubject::new(fault)?;
    let ended = case.run(&subject).await;
    let failure = ended.err().context("the fault was not caught")?;
    assert_eq!(failure.case, case);
    assert!(failure.to_string().contains(reason), "{failure}");
    Ok(())
}

/// One test per fault and the case that catches it.
macro_rules! caught {
    ($($name:ident: $fault:ident in $case:ident as $reason:literal),* $(,)?) => {
        $(
            #[tokio::test]
            async fn $name() -> Result<()> {
                catches(Fault::$fault, Case::$case, $reason).await
            }
        )*
        #[test]
        fn every_case_catches_something() {
            for case in Case::ALL {
                assert!([$(Case::$case),*].contains(&case), "{case:?}");
            }
        }
    };
}

caught! {
    a_second_turn_without_the_first_is_caught:
        Forgetful in SecondTurnContinues as "saw Some(1) user messages",
    a_resumed_session_without_its_history_is_caught:
        Forgetful in ResumesAfterRestart as "saw Some(1) user messages",
    a_new_session_in_place_of_a_lost_one_is_caught:
        NewSession in ResumeFailureIsAnError as "are [Answered, Answered], not [Answered]",
    a_timeout_that_cancels_nothing_is_caught:
        IgnoresCancel in TimeoutStopsTheTurn as "model call 0 is Some(Open)",
    a_drop_that_cancels_nothing_is_caught:
        IgnoresCancel in DropStopsTheTurn as "model call 1 is Some(Open)",
    a_turn_without_a_limit_is_caught:
        Unguarded in TimeoutStopsTheTurn as "did not end within",
    a_drop_nobody_cancels_is_caught:
        Unguarded in DropStopsTheTurn as "model call 1 is Some(Open)",
    a_turn_admitted_before_the_last_one_is_settled_is_caught:
        Unguarded in OneTurnAtATime as "are [Answered, Open, Answered]",
    a_turn_that_runs_again_is_caught:
        Unguarded in NoTurnTwice as "turn t1 ended as Completed",
    a_replayed_turn_is_caught:
        Replays in KillNeitherReplaysNorLosesTheSeat as "the killed turn did not end",
    a_lost_seat_is_caught:
        StaysDown in KillNeitherReplaysNorLosesTheSeat as "turn t3 failed",
    cumulative_usage_is_caught:
        TotalUsage in UsageIsPerTurn as "reported ([100], [130])",
    an_unverified_tool_list_is_caught:
        Unverified in ToolListIsExact as "started with a tool list that is Extra",
    an_instance_left_by_a_failed_start_is_caught:
        LeavesFailedStart in ToolListIsExact as "are left",
    an_unverified_config_is_caught:
        UnverifiedConfig in ConfigIsTheOneWritten as "started with another config",
    an_orphan_left_by_stop_is_caught:
        LeavesOrphans in NothingLeftAfterStop as "are left",
}
