//! Executable tools: at most once per call, hooks, and the autonomy gate.
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod support;

use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use support::{Hive, hook, tool};
use tinyhivemind_host::harness::{ApprovalFuture, Approver, ToolApproval, ToolDecision};
use tinyhivemind_lang::{Access, ExecTool};

fn stamp() -> ExecTool {
    tool(
        "stamp",
        "stamp",
        "write",
        json!({"type": "object", "properties": {"note": {"type": "string"}}}),
    )
}
fn look() -> ExecTool {
    tool("look", "look", "read", json!({"type": "object"}))
}
fn identified(hive: &Hive, call: &str, mut args: Value) -> Value {
    args["turn"] = json!(hive.turn_id());
    args["call"] = json!(call);
    args
}
async fn with(programs: Vec<ExecTool>, access: Access) -> Hive {
    Hive::start(
        |services| services,
        move |seat, context| {
            seat.with_context(context)
                .with_programs(programs)
                .with_access(access)
        },
    )
    .await
}

#[tokio::test]
async fn a_retried_executable_tool_runs_once_and_returns_the_first_result() {
    let hive = with(vec![stamp()], Access::Autonomous).await;
    let first = hive
        .a
        .call("stamp", identified(&hive, "s1", json!({"note": "x"})))
        .await;
    assert_eq!(first, ("stamped stamp-1\n".into(), false));
    assert_eq!(
        hive.a
            .call("stamp", identified(&hive, "s1", json!({"note": "x"})))
            .await,
        first
    );
    assert_eq!(hive.lines("stamp.log"), 1);
    // The tool saw its own arguments only: `turn` and `call` were removed.
    let logged = std::fs::read_to_string(hive.workspace.path().join("stamp.log")).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&logged).unwrap(),
        json!({"note": "x"})
    );
    let (text, failed) = hive
        .a
        .call("stamp", identified(&hive, "s1", json!({"note": "y"})))
        .await;
    assert!(failed && text.contains("already made"), "{text}");
    let second = hive
        .a
        .call("stamp", identified(&hive, "s2", json!({"note": "x"})))
        .await;
    assert_eq!(second, ("stamped stamp-2\n".into(), false));
    assert_eq!(hive.lines("stamp.log"), 2);
}

#[tokio::test]
async fn an_executable_tool_retried_while_its_outcome_is_unknown_is_refused_and_runs_once() {
    let hive = with(
        vec![tool("slow", "slow", "write", json!({"type": "object"}))],
        Access::Autonomous,
    )
    .await;
    let first = tokio::spawn({
        let (a, args) = (hive.a.clone(), identified(&hive, "s1", json!({})));
        async move { a.call("slow", args).await }
    });
    while hive.lines("slow.log") == 0 {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let (text, failed) = hive
        .a
        .call("slow", identified(&hive, "s1", json!({})))
        .await;
    assert!(failed && text.contains("outcome is not known"), "{text}");
    std::fs::write(hive.workspace.path().join("release"), "").unwrap();
    assert_eq!(first.await.unwrap(), ("released\n".into(), false));
    assert_eq!(hive.lines("slow.log"), 1);
    assert_eq!(
        hive.a
            .call("slow", identified(&hive, "s1", json!({})))
            .await,
        ("released\n".into(), false)
    );
    assert_eq!(hive.lines("slow.log"), 1);
}

#[tokio::test]
async fn a_read_tool_without_a_hook_needs_no_identity_and_passes_every_gate() {
    let hive = with(vec![look()], Access::ReadOnly).await;
    let tools = hive.a.tools().await;
    let schema = &tools.iter().find(|(name, _)| name == "look").unwrap().1;
    assert!(schema["properties"].get("call").is_none(), "{schema}");
    assert_eq!(
        hive.a.call("look", json!({})).await,
        ("seen\n".into(), false)
    );
}

#[tokio::test]
async fn a_retried_read_with_a_hook_reruns_neither() {
    let hive = Hive::start(
        |services| services,
        |seat, context| {
            seat.with_context(context)
                .with_programs(vec![look()])
                .with_hooks(vec![
                    hook("guard", "pre_tool_use", "look", "guard"),
                    hook("audit", "post_tool_use", "look", "audit"),
                ])
        },
    )
    .await;
    let tools = hive.a.tools().await;
    let schema = &tools.iter().find(|(name, _)| name == "look").unwrap().1;
    assert!(
        schema["properties"].get("call").is_some(),
        "a hooked read changes state: {schema}"
    );
    let first = hive
        .a
        .call("look", identified(&hive, "l1", json!({})))
        .await;
    assert_eq!(first, ("seen\n".into(), false));
    assert_eq!(
        hive.a
            .call("look", identified(&hive, "l1", json!({})))
            .await,
        first
    );
    assert_eq!(
        (
            hive.lines("look.log"),
            hive.lines("guard.log"),
            hive.lines("audit.log")
        ),
        (1, 1, 1)
    );
    let audited: Value = serde_json::from_str(
        &std::fs::read_to_string(hive.workspace.path().join("audit.log")).unwrap(),
    )
    .unwrap();
    assert_eq!(audited["result"], "seen\n");
    assert_eq!(audited["arguments"], json!({}));
}

#[tokio::test]
async fn a_package_hook_denies_a_call_before_it_runs() {
    let hive = Hive::start(
        |services| services,
        |seat, context| {
            seat.with_context(context)
                .with_programs(vec![stamp()])
                .with_access(Access::Autonomous)
                .with_hooks(vec![hook("guard", "pre_tool_use", "st*", "guard")])
        },
    )
    .await;
    let denied = hive
        .a
        .call(
            "stamp",
            identified(&hive, "s1", json!({"note": "forbidden"})),
        )
        .await;
    assert_eq!(denied, ("forbidden by guard".into(), true));
    assert_eq!(hive.lines("stamp.log"), 0);
    assert_eq!(
        hive.a
            .call(
                "stamp",
                identified(&hive, "s1", json!({"note": "forbidden"}))
            )
            .await,
        denied
    );
    assert_eq!(hive.lines("guard.log"), 1);
    let (_, failed) = hive
        .a
        .call("stamp", identified(&hive, "s2", json!({"note": "fine"})))
        .await;
    assert!(!failed);
    assert_eq!(hive.lines("stamp.log"), 1);
}

#[tokio::test]
async fn read_only_refuses_an_effectful_tool() {
    let hive = with(vec![stamp(), look()], Access::ReadOnly).await;
    let (text, failed) = hive
        .a
        .call("stamp", identified(&hive, "s1", json!({})))
        .await;
    assert!(failed && text.contains("read_only"), "{text}");
    assert_eq!(hive.lines("stamp.log"), 0);
    // Refused by policy before any hook: nothing recorded, so a retry is judged again.
    let (_, failed) = hive
        .a
        .call("stamp", identified(&hive, "s1", json!({})))
        .await;
    assert!(failed);
    // Hive tools stay under send policy, not the access gate.
    let post =
        json!({"episode_id": hive.episode(), "body": "b", "turn": hive.turn_id(), "call": "p1"});
    assert!(!hive.a.call("hivemind_post", post).await.1);
}

#[tokio::test]
async fn supervised_without_an_approver_refuses_an_effectful_tool() {
    let hive = with(vec![stamp()], Access::Supervised).await;
    let (text, failed) = hive
        .a
        .call("stamp", identified(&hive, "s1", json!({})))
        .await;
    assert!(failed && text.contains("no approver"), "{text}");
    assert_eq!(hive.lines("stamp.log"), 0);
}

/// Approves notes other than "no", and remembers what it was asked.
#[derive(Default)]
struct Judge(Mutex<Vec<ToolApproval>>);
impl Approver for Judge {
    fn decide(&self, request: ToolApproval) -> ApprovalFuture {
        let refuse = request.arguments["note"] == "no";
        self.0.lock().unwrap().push(request);
        Box::pin(async move {
            if refuse {
                ToolDecision::Deny {
                    reason: "the operator said no".into(),
                }
            } else {
                ToolDecision::Allow
            }
        })
    }
}

#[tokio::test]
async fn supervised_asks_the_hosts_approver() {
    let judge = Arc::new(Judge::default());
    let hive = Hive::start(
        {
            let judge = judge.clone();
            move |services| services.with_approver(judge)
        },
        |seat, context| seat.with_context(context).with_programs(vec![stamp()]),
    )
    .await;
    assert!(
        !hive
            .a
            .call("stamp", identified(&hive, "s1", json!({"note": "yes"})))
            .await
            .1
    );
    let (text, failed) = hive
        .a
        .call("stamp", identified(&hive, "s2", json!({"note": "no"})))
        .await;
    assert!(failed && text.contains("the operator said no"), "{text}");
    assert_eq!(hive.lines("stamp.log"), 1);
    let asked = judge.0.lock().unwrap().clone();
    assert_eq!(asked.len(), 2);
    assert_eq!(
        asked[0],
        ToolApproval {
            seat: "a".into(),
            turn_id: hive.turn_id(),
            call_id: "s1".into(),
            tool: "stamp".into(),
            arguments: json!({"note": "yes"}),
        }
    );
}

#[tokio::test]
async fn a_failing_tool_is_an_error_result_answered_from_its_record() {
    let broken = tool("broken", "missing", "write", json!({"type": "object"}));
    let hive = with(vec![broken], Access::Autonomous).await;
    let first = hive
        .a
        .call("broken", identified(&hive, "b1", json!({})))
        .await;
    assert!(first.1, "{first:?}");
    assert_eq!(
        hive.a
            .call("broken", identified(&hive, "b1", json!({})))
            .await,
        first
    );
}
