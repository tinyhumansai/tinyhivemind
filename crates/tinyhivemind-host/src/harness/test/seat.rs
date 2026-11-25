//! One turn at a time, one session for life, and no turn twice.
use super::support::{Handle, Scripted, Turn, request};
use crate::harness::SeatRunner;
use anyhow::{Context as _, Result};
use std::sync::Arc;
use std::time::Duration;
use tinyhivemind_hives::AgentRunner;

fn seat(inner: &Arc<Scripted>) -> SeatRunner {
    SeatRunner::new(Arc::new(Handle(inner.clone())))
}

/// Yield until the log is `expected`.
async fn logged(inner: &Scripted, expected: &[&str]) {
    let reached = async {
        while inner.log() != expected {
            tokio::task::yield_now().await;
        }
    };
    let waited = tokio::time::timeout(Duration::from_secs(30), reached).await;
    assert!(
        waited.is_ok(),
        "{:?} never became {expected:?}",
        inner.log()
    );
}

async fn refusal(seat: &SeatRunner, turn_id: &str, session: Option<&str>) -> Result<String> {
    let ended = seat.run(request(turn_id, session)).await;
    Ok(ended.err().context("the turn was not refused")?.to_string())
}

#[tokio::test]
async fn a_second_turn_waits_for_the_first_and_its_cancellation() -> Result<()> {
    let inner = Scripted::new(&[Turn::Hang, Turn::Session("s")]);
    inner.permits.forget_permits(usize::MAX);
    let seat = seat(&inner);
    let first = tokio::spawn(seat.run(request("t1", None)));
    logged(&inner, &["run t1 none"]).await;
    let second = tokio::spawn(seat.run(request("t2", None)));
    first.abort();
    logged(&inner, &["run t1 none", "cancel t1"]).await;
    for _ in 0..100 {
        tokio::task::yield_now().await;
    }
    assert!(!second.is_finished());
    inner.permits.add_permits(1);
    assert_eq!(second.await??.session_id, "s");
    let order = ["run t1 none", "cancel t1", "cancelled t1", "run t2 none"];
    assert_eq!(inner.log(), order);
    Ok(())
}

#[tokio::test]
async fn a_failed_turn_is_cancelled_before_its_error_is_returned() -> Result<()> {
    let inner = Scripted::new(&[Turn::Fail, Turn::Session("s")]);
    let seat = seat(&inner);
    assert!(refusal(&seat, "t1", None).await?.contains("scripted"));
    assert_eq!(inner.log(), ["run t1 none", "cancel t1", "cancelled t1"]);
    seat.run(request("t2", None)).await?;
    Ok(())
}

#[tokio::test]
async fn a_turn_past_its_timeout_is_cancelled_and_the_seat_stays_usable() -> Result<()> {
    let inner = Scripted::new(&[Turn::Session("s"), Turn::Hang, Turn::Session("s")]);
    let seat = seat(&inner).with_timeout(Some(Duration::from_millis(50)));
    seat.run(request("t1", None)).await?;
    assert!(refusal(&seat, "t2", Some("s")).await?.contains("50ms"));
    assert!(inner.log().contains(&"cancelled t2".to_owned()));
    seat.run(request("t3", Some("s"))).await?;
    Ok(())
}

#[tokio::test]
async fn a_turn_that_cannot_be_shown_to_have_stopped_blocks_the_seat_until_it_is() -> Result<()> {
    let inner = Scripted::new(&[Turn::Fail, Turn::Session("s")]);
    inner.refuse_cancel(true);
    let seat = seat(&inner);
    assert!(refusal(&seat, "t1", None).await?.contains("will not stop"));
    let blocked = refusal(&seat, "t2", None).await?;
    assert!(blocked.contains("t1"), "{blocked}");
    assert!(!inner.log().iter().any(|entry| entry.starts_with("run t2")));
    inner.refuse_cancel(false);
    // Refused before it started, so this is not a replay.
    seat.run(request("t2", None)).await?;
    Ok(())
}

#[tokio::test]
async fn an_abandoned_turn_whose_cancellation_failed_is_cancelled_again() -> Result<()> {
    let inner = Scripted::new(&[Turn::Hang, Turn::Session("s")]);
    inner.refuse_cancel(true);
    let seat = seat(&inner);
    let first = tokio::spawn(seat.run(request("t1", None)));
    logged(&inner, &["run t1 none"]).await;
    first.abort();
    logged(&inner, &["run t1 none", "cancel t1"]).await;
    inner.refuse_cancel(false);
    seat.run(request("t2", None)).await?;
    assert!(inner.log().contains(&"run t2 none".to_owned()));
    Ok(())
}

#[test]
fn a_turn_abandoned_with_its_runtime_is_cancelled_before_the_next() -> Result<()> {
    let inner = Scripted::new(&[Turn::Hang, Turn::Session("s")]);
    let seat = seat(&inner);
    let mut first = seat.run(request("t1", None));
    // Dropped outside any runtime, so there is nothing to cancel it on.
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(async {
        let polled = tokio::time::timeout(Duration::from_millis(10), &mut first);
        assert!(polled.await.is_err());
    });
    drop(runtime);
    drop(first);
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.block_on(seat.run(request("t2", None)))?;
    let order = ["run t1 none", "cancel t1", "cancelled t1", "run t2 none"];
    assert_eq!(inner.log(), order);
    Ok(())
}

#[tokio::test]
async fn a_turn_id_runs_once_whatever_became_of_it() -> Result<()> {
    let inner = Scripted::new(&[Turn::Session("s"), Turn::Fail, Turn::Hang]);
    let seat = seat(&inner);
    seat.run(request("t1", None)).await?;
    let _ = seat.run(request("t2", Some("s"))).await;
    let third = tokio::spawn(seat.run(request("t3", Some("s"))));
    while !inner.log().contains(&"run t3 s".to_owned()) {
        tokio::task::yield_now().await;
    }
    third.abort();
    for turn in ["t1", "t2", "t3"] {
        let refused = refusal(&seat, turn, Some("s")).await?;
        assert!(refused.contains("already started"), "{refused}");
    }
    assert!(refusal(&seat, "", Some("s")).await?.contains("no turn id"));
    let runs = inner
        .log()
        .iter()
        .filter(|entry| entry.starts_with("run"))
        .count();
    assert_eq!(runs, 3);
    Ok(())
}

#[tokio::test]
async fn a_turn_dropped_while_it_waited_never_started() -> Result<()> {
    let inner = Scripted::new(&[Turn::Hang, Turn::Session("s")]);
    let seat = seat(&inner);
    let first = tokio::spawn(seat.run(request("t1", None)));
    logged(&inner, &["run t1 none"]).await;
    let mut waiting = seat.run(request("t2", None));
    let polled = tokio::time::timeout(Duration::from_millis(10), &mut waiting).await;
    assert!(polled.is_err());
    drop(waiting);
    first.abort();
    seat.run(request("t2", None)).await?;
    Ok(())
}

#[tokio::test]
async fn an_empty_session_is_an_error() -> Result<()> {
    let inner = Scripted::new(&[Turn::Session(" ")]);
    let refused = refusal(&seat(&inner), "t1", None).await?;
    assert!(refused.contains("empty session"), "{refused}");
    Ok(())
}

#[tokio::test]
async fn a_changed_session_is_an_error_and_the_seat_keeps_its_own() -> Result<()> {
    let inner = Scripted::new(&[
        Turn::Session("s"),
        Turn::Session("other"),
        Turn::Session("s"),
    ]);
    let seat = seat(&inner);
    seat.run(request("t1", None)).await?;
    let refused = refusal(&seat, "t2", Some("s")).await?;
    assert!(refused.contains("other"), "{refused}");
    seat.run(request("t3", Some("s"))).await?;
    Ok(())
}

#[tokio::test]
async fn a_request_for_another_session_never_reaches_the_harness() -> Result<()> {
    let inner = Scripted::new(&[Turn::Session("s")]);
    let seat = seat(&inner);
    seat.run(request("t1", Some("s"))).await?;
    let refused = refusal(&seat, "t2", Some("other")).await?;
    assert!(refused.contains("other"), "{refused}");
    assert_eq!(inner.log().len(), 1);
    Ok(())
}

#[tokio::test]
async fn a_resumed_session_must_come_back_as_itself() -> Result<()> {
    let inner = Scripted::new(&[Turn::Session("new")]);
    let refused = refusal(&seat(&inner), "t1", Some("old")).await?;
    assert!(
        refused.contains("old") && refused.contains("new"),
        "{refused}"
    );
    Ok(())
}

#[tokio::test]
async fn a_request_without_a_session_continues_the_one_the_seat_has() -> Result<()> {
    let inner = Scripted::new(&[Turn::Session("s"), Turn::Session("s")]);
    let seat = seat(&inner);
    seat.run(request("t1", None)).await?;
    seat.run(request("t2", None)).await?;
    assert_eq!(inner.log(), ["run t1 none", "run t2 s"]);
    Ok(())
}
