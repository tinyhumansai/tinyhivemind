//! Defects a security review found: turn changes, hang-ups, bounds, schemas.
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod support;

use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use support::{Hive, hook, raw_post, started, tool};
use tinyhivemind_host::harness::{ApprovalFuture, Approver, ToolApproval, ToolDecision};
use tinyhivemind_lang::{Access, ExecTool};
use tinyhivemind_mcp::ServerOptions;
use tokio::sync::Notify;

fn stamp() -> ExecTool {
    tool(
        "stamp",
        "stamp",
        "write",
        json!({"type": "object", "properties": {"note": {"type": "string"}}, "required": ["note"], "additionalProperties": false}),
    )
}
fn slow() -> ExecTool {
    tool("slow", "slow", "write", json!({"type": "object"}))
}
fn identified(hive: &Hive, call: &str, mut args: Value) -> Value {
    args["turn"] = json!(hive.turn_id());
    args["call"] = json!(call);
    args
}
fn call_message(name: &str, arguments: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": 9, "method": "tools/call", "params": {"name": name, "arguments": arguments}})
}

/// Approves everything, holding a note "hold" until released; counts asks.
#[derive(Clone, Default)]
struct Gate {
    asked: Arc<AtomicUsize>,
    waiting: Arc<Notify>,
    release: Arc<Notify>,
}
impl Approver for Gate {
    fn decide(&self, request: ToolApproval) -> ApprovalFuture {
        self.asked.fetch_add(1, Ordering::SeqCst);
        let gate = self.clone();
        Box::pin(async move {
            if request.arguments["note"] == "hold" {
                gate.waiting.notify_one();
                gate.release.notified().await;
            }
            ToolDecision::Allow
        })
    }
}
async fn gated(gate: &Gate, options: ServerOptions) -> Hive {
    let approver = gate.clone();
    Hive::start_with(
        move |services| services.with_approver(Arc::new(approver)),
        |seat, context| seat.with_context(context).with_programs(vec![stamp(), slow()]),
        options,
    )
    .await
}

#[tokio::test]
async fn an_approval_in_flight_across_a_turn_change_runs_nothing_and_keeps_the_new_turns_record() {
    let gate = Gate::default();
    let mut hive = gated(&gate, ServerOptions::default()).await;
    let held = tokio::spawn({
        let (a, args) = (hive.a.clone(), identified(&hive, "s1", json!({"note": "hold"})));
        async move { a.call("stamp", args).await }
    });
    gate.waiting.notified().await;
    hive.next_turn().await;
    let fresh = hive.a.call("stamp", identified(&hive, "s1", json!({"note": "new"}))).await;
    assert!(!fresh.1, "{fresh:?}");
    gate.release.notify_one();
    let (text, failed) = held.await.unwrap();
    assert!(failed && text.contains("not the running turn"), "{text}");
    assert_eq!(hive.lines("stamp.log"), 1);
    // The new turn's record survived the stale call.
    let again = hive.a.call("stamp", identified(&hive, "s1", json!({"note": "new"}))).await;
    assert_eq!(again, fresh);
    assert_eq!(hive.lines("stamp.log"), 1);
}

#[tokio::test]
async fn a_retry_while_approval_is_pending_does_not_ask_twice() {
    let gate = Gate::default();
    let hive = gated(&gate, ServerOptions::default()).await;
    let held = tokio::spawn({
        let (a, args) = (hive.a.clone(), identified(&hive, "s1", json!({"note": "hold"})));
        async move { a.call("stamp", args).await }
    });
    gate.waiting.notified().await;
    let (text, failed) = hive.a.call("stamp", identified(&hive, "s1", json!({"note": "hold"}))).await;
    assert!(failed && text.contains("outcome is not known"), "{text}");
    assert_eq!(gate.asked.load(Ordering::SeqCst), 1);
    gate.release.notify_one();
    assert!(!held.await.unwrap().1);
    assert_eq!(hive.lines("stamp.log"), 1);
}

#[tokio::test]
async fn a_client_hanging_up_mid_call_leaves_one_run_and_the_retry_gets_its_result() {
    let hive = Hive::start(|services| services, |seat, context| {
        seat.with_context(context).with_programs(vec![slow()]).with_access(Access::Autonomous)
    })
    .await;
    let addr = hive.server.local_addr();
    let socket = raw_post(addr, &hive.a.bearer, &call_message("slow", identified(&hive, "s1", json!({})))).await;
    started(&hive, "slow.log").await;
    drop(socket);
    std::fs::write(hive.workspace.path().join("release"), "").unwrap();
    let answer = loop {
        let answer = hive.a.call("slow", identified(&hive, "s1", json!({}))).await;
        if !answer.0.contains("outcome is not known") {
            break answer;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(answer, ("released\n".into(), false));
    assert_eq!(hive.lines("slow.log"), 1);
}

#[tokio::test]
async fn calls_in_flight_are_bounded_per_seat_even_after_a_hang_up() {
    let options = ServerOptions {
        max_calls_per_seat: 1,
        ..ServerOptions::default()
    };
    let hive = Hive::start_with(
        |services| services,
        |seat, context| {
            seat.with_context(context)
                .with_programs(vec![slow(), tool("look", "look", "read", json!({"type": "object"}))])
                .with_access(Access::Autonomous)
        },
        options,
    )
    .await;
    let addr = hive.server.local_addr();
    let socket = raw_post(addr, &hive.a.bearer, &call_message("slow", identified(&hive, "s1", json!({})))).await;
    started(&hive, "slow.log").await;
    drop(socket);
    let waiting = tokio::spawn({
        let a = hive.a.clone();
        async move { a.call("look", json!({})).await }
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!waiting.is_finished());
    assert_eq!(hive.lines("look.log"), 0);
    std::fs::write(hive.workspace.path().join("release"), "").unwrap();
    assert_eq!(waiting.await.unwrap(), ("seen\n".into(), false));
}

#[tokio::test]
async fn arguments_outside_the_declared_schema_are_refused_before_anything_runs() {
    let gate = Gate::default();
    let approver = gate.clone();
    let hive = Hive::start(
        move |services| services.with_approver(Arc::new(approver)),
        |seat, context| {
            seat.with_context(context)
                .with_programs(vec![stamp()])
                .with_hooks(vec![hook("guard", "pre_tool_use", "stamp", "guard")])
        },
    )
    .await;
    for (args, field) in [
        (json!({"note": 5}), "note"),
        (json!({}), "note"),
        (json!({"note": "x", "extra": 1}), "extra"),
    ] {
        let (text, failed) = hive.a.call("stamp", identified(&hive, "s1", args)).await;
        assert!(failed && text.contains(field), "{text}");
    }
    assert_eq!((hive.lines("guard.log"), hive.lines("stamp.log")), (0, 0));
    assert_eq!(gate.asked.load(Ordering::SeqCst), 0);
    // Nothing was recorded, so the same call with fitting arguments acts.
    let (text, failed) = hive.a.call("stamp", identified(&hive, "s1", json!({"note": "x"}))).await;
    assert!(!failed, "{text}");
    assert_eq!(hive.lines("stamp.log"), 1);
}

#[tokio::test]
async fn a_seats_records_are_bounded_and_refused_rather_than_evicted() {
    let options = ServerOptions {
        max_records: 2,
        max_record_bytes: 4096,
        ..ServerOptions::default()
    };
    let hive = Hive::start_with(
        |services| services,
        |seat, context| seat.with_context(context).with_programs(vec![stamp()]).with_access(Access::Autonomous),
        options,
    )
    .await;
    for call in ["s1", "s2"] {
        assert!(!hive.a.call("stamp", identified(&hive, call, json!({"note": "x"}))).await.1);
    }
    let (text, failed) = hive.a.call("stamp", identified(&hive, "s3", json!({"note": "x"}))).await;
    assert!(failed && text.contains("records"), "{text}");
    let big = "x".repeat(5000);
    let (text, failed) = hive.a.call("stamp", identified(&hive, "s1", json!({"note": big}))).await;
    assert!(failed, "{text}");
    // The first record is still there, answered without running.
    assert!(!hive.a.call("stamp", identified(&hive, "s1", json!({"note": "x"}))).await.1);
    assert_eq!(hive.lines("stamp.log"), 2);
}
