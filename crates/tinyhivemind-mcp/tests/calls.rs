//! Hive tools: the turn check, call identity, and policy before dispatch.
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod support;

use serde_json::{Value, json};
use std::sync::Arc;
use support::{Hive, hook};
use tinyhivemind_hives::HiveInfo;
use tinyhivemind_host::Result as HostResult;
use tinyhivemind_host::policy::{
    AgentCreator, CreateFuture, ManagementAuthorizer, ManagementRequest, NewAgent, SendAuthorizer,
    SendRequest,
};

fn post(hive: &Hive, call: &str, body: &str) -> Value {
    json!({"episode_id": hive.episode(), "body": body, "turn": hive.turn_id(), "call": call})
}

async fn plain() -> Hive {
    Hive::start(|services| services, |seat, _| seat).await
}

#[tokio::test]
async fn tools_list_adds_turn_and_call_to_state_changing_tools_only() {
    let hive = plain().await;
    let tools = hive.a.tools().await;
    let names: Vec<&str> = tools.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        [
            "hivemind_list_hives",
            "hivemind_list_agents",
            "hivemind_read",
            "hivemind_send_hive",
            "hivemind_send_agent",
            "hivemind_post",
            "hivemind_ask",
            "hivemind_broadcast",
            "hivemind_complete",
        ]
    );
    for (name, schema) in &tools {
        let changes = !matches!(
            name.as_str(),
            "hivemind_list_hives" | "hivemind_list_agents" | "hivemind_read"
        );
        for field in ["turn", "call"] {
            assert_eq!(
                schema["properties"].get(field).is_some(),
                changes,
                "{name} {field}"
            );
            let required = schema["required"].as_array().unwrap();
            assert_eq!(required.contains(&json!(field)), changes, "{name} {field}");
        }
    }
    let names: Vec<String> = hive.endpoint.tools.iter().cloned().collect();
    assert_eq!(names.len(), 9);
}

#[tokio::test]
async fn a_retried_post_acts_once_and_returns_the_first_result() {
    let mut hive = plain().await;
    let first = hive
        .a
        .call("hivemind_post", post(&hive, "p1", "once"))
        .await;
    assert_eq!(first, (r#"{"accepted":true}"#.into(), false));
    assert_eq!(
        hive.a
            .call("hivemind_post", post(&hive, "p1", "once"))
            .await,
        first
    );
    hive.settle().await;
    assert_eq!(hive.written("a", "once"), 1);
}

#[tokio::test]
async fn two_identical_posts_under_different_calls_both_act() {
    let mut hive = plain().await;
    for call in ["p1", "p2"] {
        let (_, failed) = hive
            .a
            .call("hivemind_post", post(&hive, call, "twice"))
            .await;
        assert!(!failed);
    }
    hive.settle().await;
    assert_eq!(hive.written("a", "twice"), 2);
}

#[tokio::test]
async fn reusing_a_call_with_other_arguments_or_another_tool_is_refused() {
    let mut hive = plain().await;
    hive.a
        .call("hivemind_post", post(&hive, "p1", "first"))
        .await;
    let (text, failed) = hive
        .a
        .call("hivemind_post", post(&hive, "p1", "second"))
        .await;
    assert!(failed && text.contains("already made"), "{text}");
    let (text, failed) = hive
        .a
        .call("hivemind_complete", post(&hive, "p1", "first"))
        .await;
    assert!(failed && text.contains("already made"), "{text}");
    hive.settle().await;
    assert_eq!(hive.written("a", "second"), 0);
}

#[tokio::test]
async fn a_missing_call_or_turn_is_refused() {
    let mut hive = plain().await;
    let mut missing_call = post(&hive, "p1", "none");
    missing_call.as_object_mut().unwrap().remove("call");
    let (text, failed) = hive.a.call("hivemind_post", missing_call).await;
    assert!(failed && text.contains("call"), "{text}");
    let mut missing_turn = post(&hive, "p1", "none");
    missing_turn.as_object_mut().unwrap().remove("turn");
    let (text, failed) = hive.a.call("hivemind_post", missing_turn).await;
    assert!(failed && text.contains("turn"), "{text}");
    let (_, failed) = hive.a.call("hivemind_post", post(&hive, "", "none")).await;
    assert!(failed);
    hive.settle().await;
    assert_eq!(hive.written("a", "none"), 0);
}

#[tokio::test]
async fn a_stale_turn_is_refused() {
    let mut hive = plain().await;
    let mut stale = post(&hive, "p1", "late");
    stale["turn"] = json!("0:1:a");
    let (text, failed) = hive.a.call("hivemind_post", stale).await;
    assert!(failed && text.contains("not the running turn"), "{text}");
    hive.settle().await;
    // After the turn ends, its own id is stale too.
    let (_, failed) = hive
        .a
        .call("hivemind_post", post(&hive, "p2", "late"))
        .await;
    assert!(failed);
    assert_eq!(hive.written("a", "late"), 0);
}

#[tokio::test]
async fn another_seats_bearer_cannot_act_in_a_turn_and_has_none_of_its_own() {
    let mut hive = plain().await;
    // `b` presents `a`'s turn and episode: the bearer says `b`, which is not running.
    let (text, failed) = hive
        .b
        .call("hivemind_post", post(&hive, "p1", "as b"))
        .await;
    assert!(
        failed && text.contains("not the running turn of b"),
        "{text}"
    );
    hive.settle().await;
    assert_eq!(hive.written("a", "as b") + hive.written("b", "as b"), 0);
}

#[tokio::test]
async fn another_seats_episode_is_refused() {
    let mut hive = plain().await;
    let mut other = post(&hive, "p1", "elsewhere");
    other["episode_id"] = json!("episode:999");
    let (text, failed) = hive.a.call("hivemind_post", other).await;
    assert!(failed && text.contains("stale episode"), "{text}");
    hive.settle().await;
    assert_eq!(hive.written("a", "elsewhere"), 0);
}

#[tokio::test]
async fn a_forged_actor_is_refused() {
    let mut hive = plain().await;
    let mut forged = post(&hive, "p1", "forged");
    forged["sender"] = json!("b");
    let (text, failed) = hive.a.call("hivemind_post", forged).await;
    assert!(failed && text.contains("unknown argument sender"), "{text}");
    hive.settle().await;
    assert_eq!(hive.written("a", "forged") + hive.written("b", "forged"), 0);
}

#[tokio::test]
async fn an_unknown_tool_is_a_protocol_error() {
    let hive = plain().await;
    let response = hive
        .a
        .rpc(
            "tools/call",
            json!({"name": "hivemind_create_hive", "arguments": {}}),
        )
        .await;
    assert_eq!(response["error"]["code"], -32602);
}

#[tokio::test]
async fn a_read_needs_no_identity() {
    let hive = plain().await;
    let (text, failed) = hive.a.call("hivemind_list_hives", json!({})).await;
    assert!(!failed, "{text}");
    assert!(text.contains("work"));
}

#[tokio::test]
async fn a_retried_send_acts_once_and_one_message_id_is_one_message() {
    let hive = plain().await;
    let send = |call: &str| json!({"agent_id": "b", "message_id": "m1", "body": "hello b", "turn": hive.turn_id(), "call": call});
    let first = hive.a.call("hivemind_send_agent", send("s1")).await;
    assert!(!first.1, "{first:?}");
    assert_eq!(hive.a.call("hivemind_send_agent", send("s1")).await, first);
    assert_eq!(hive.a.call("hivemind_send_agent", send("s2")).await, first);
    assert_eq!(
        hive.coordinator.read_direct("a", "b", None).unwrap().len(),
        1
    );
}

/// Admits every management request; builds no agents.
struct Open;
impl ManagementAuthorizer for Open {
    fn authorize(&self, _: &str, _: &ManagementRequest) -> HostResult<()> {
        Ok(())
    }
}
impl AgentCreator for Open {
    fn create(&self, _: NewAgent) -> CreateFuture<'_> {
        Box::pin(async { Err(tinyhivemind_host::Error::Host("no templates".into())) })
    }
}

#[tokio::test]
async fn a_retried_join_acts_once_and_returns_the_first_result() {
    let hive = Hive::start(
        |services| services.with_management(Arc::new(Open), Arc::new(Open)),
        |seat, _| seat,
    )
    .await;
    assert_eq!(hive.a.tools().await.len(), 13);
    hive.coordinator
        .create_hive(HiveInfo {
            hive_id: "side".into(),
            name: "Side".into(),
            description: None,
            members: Vec::new(),
        })
        .await
        .unwrap();
    let join = json!({"hive_id": "side", "agent_id": "a", "turn": hive.turn_id(), "call": "j1"});
    let first = hive.a.call("hivemind_join_hive", join.clone()).await;
    assert_eq!(first, (r#"{"joined":true}"#.into(), false));
    hive.coordinator.leave_hive("side", "a").await.unwrap();
    assert_eq!(hive.a.call("hivemind_join_hive", join).await, first);
    let side = hive
        .coordinator
        .list_hives()
        .unwrap()
        .into_iter()
        .find(|h| h.hive_id == "side")
        .unwrap();
    assert_eq!(side.members, Vec::<String>::new());
}

#[tokio::test]
async fn a_failed_agent_creation_is_recorded_and_not_retried() {
    let hive = Hive::start(
        |services| services.with_management(Arc::new(Open), Arc::new(Open)),
        |seat, _| seat,
    )
    .await;
    let create = json!({"template": "t", "config": {}, "turn": hive.turn_id(), "call": "c1"});
    let first = hive.a.call("hivemind_create_agent", create.clone()).await;
    assert!(first.1 && first.0.contains("no templates"), "{first:?}");
    assert_eq!(hive.a.call("hivemind_create_agent", create).await, first);
}

#[tokio::test]
async fn a_hook_denied_call_is_recorded_and_a_corrected_call_uses_a_new_call() {
    let mut hive = Hive::start(
        |services| services,
        |seat, context| {
            seat.with_context(context).with_hooks(vec![hook(
                "guard",
                "pre_tool_use",
                "hivemind_post",
                "guard",
            )])
        },
    )
    .await;
    let denied = hive
        .a
        .call("hivemind_post", post(&hive, "p1", "forbidden"))
        .await;
    assert_eq!(denied, ("forbidden by guard".into(), true));
    assert_eq!(hive.lines("guard.log"), 1);
    // The retry is answered from the record: no hook runs again.
    assert_eq!(
        hive.a
            .call("hivemind_post", post(&hive, "p1", "forbidden"))
            .await,
        denied
    );
    assert_eq!(hive.lines("guard.log"), 1);
    let corrected = hive
        .a
        .call("hivemind_post", post(&hive, "p2", "allowed"))
        .await;
    assert!(!corrected.1, "{corrected:?}");
    assert_eq!(
        hive.a
            .call("hivemind_post", post(&hive, "p2", "allowed"))
            .await,
        corrected
    );
    assert_eq!(hive.lines("guard.log"), 2);
    hive.settle().await;
    assert_eq!(hive.written("a", "forbidden"), 0);
    assert_eq!(hive.written("a", "allowed"), 1);
}

/// Refuses direct messages whose body says "secret", and joins to `closed`.
struct Strict;
impl SendAuthorizer for Strict {
    fn authorize(&self, _: &str, request: &SendRequest) -> HostResult<()> {
        match request {
            SendRequest::Agent { body, .. } if body.contains("secret") => {
                Err(tinyhivemind_host::Error::SendDenied("no secrets".into()))
            }
            _ => Ok(()),
        }
    }
}
impl ManagementAuthorizer for Strict {
    fn authorize(&self, _: &str, request: &ManagementRequest) -> HostResult<()> {
        match request {
            ManagementRequest::JoinHive { hive_id, .. } if hive_id == "closed" => {
                Err(tinyhivemind_host::Error::Unauthorized("closed hive".into()))
            }
            _ => Ok(()),
        }
    }
}

async fn guarded() -> Hive {
    Hive::start(
        |services| {
            services
                .with_send_policy(Arc::new(Strict))
                .with_management(Arc::new(Strict), Arc::new(Open))
        },
        |seat, context| {
            seat.with_context(context).with_hooks(vec![
                hook("guard", "pre_tool_use", "hivemind_*", "guard"),
                hook("audit", "post_tool_use", "hivemind_*", "audit"),
            ])
        },
    )
    .await
}

#[tokio::test]
async fn a_send_the_policy_refuses_runs_no_hook_and_records_nothing() {
    let hive = guarded().await;
    let send = |call: &str, body: &str| json!({"agent_id": "b", "message_id": call, "body": body, "turn": hive.turn_id(), "call": call});
    let (text, failed) = hive
        .a
        .call("hivemind_send_agent", send("s1", "a secret"))
        .await;
    assert!(failed && text.contains("no secrets"), "{text}");
    assert_eq!((hive.lines("guard.log"), hive.lines("audit.log")), (0, 0));
    // Nothing was recorded: the same call is judged again, not answered.
    let (_, failed) = hive
        .a
        .call("hivemind_send_agent", send("s1", "a secret"))
        .await;
    assert!(failed);
    assert_eq!((hive.lines("guard.log"), hive.lines("audit.log")), (0, 0));
    assert_eq!(
        hive.coordinator.read_direct("a", "b", None).unwrap().len(),
        0
    );
    // An allowed send runs each hook once, and its retry runs none.
    let (text, failed) = hive
        .a
        .call("hivemind_send_agent", send("s2", "hello"))
        .await;
    assert!(!failed, "{text}");
    hive.a
        .call("hivemind_send_agent", send("s2", "hello"))
        .await;
    assert_eq!((hive.lines("guard.log"), hive.lines("audit.log")), (1, 1));
    assert_eq!(
        hive.coordinator.read_direct("a", "b", None).unwrap().len(),
        1
    );
}

#[tokio::test]
async fn a_join_management_refuses_runs_no_hook_and_records_nothing() {
    let hive = guarded().await;
    for id in ["closed", "open"] {
        hive.coordinator
            .create_hive(HiveInfo {
                hive_id: id.into(),
                name: id.into(),
                description: None,
                members: Vec::new(),
            })
            .await
            .unwrap();
    }
    let join = |call: &str, hive_id: &str| json!({"hive_id": hive_id, "agent_id": "a", "turn": hive.turn_id(), "call": call});
    let (text, failed) = hive
        .a
        .call("hivemind_join_hive", join("j1", "closed"))
        .await;
    assert!(failed && text.contains("closed hive"), "{text}");
    assert_eq!(hive.lines("guard.log"), 0);
    let (text, failed) = hive.a.call("hivemind_join_hive", join("j2", "open")).await;
    assert!(!failed, "{text}");
    assert_eq!((hive.lines("guard.log"), hive.lines("audit.log")), (1, 1));
}

/// Admits sends until told to refuse them.
#[derive(Default)]
struct Toggle(std::sync::atomic::AtomicBool);
impl SendAuthorizer for Toggle {
    fn authorize(&self, _: &str, _: &SendRequest) -> HostResult<()> {
        if self.0.load(std::sync::atomic::Ordering::SeqCst) {
            Err(tinyhivemind_host::Error::SendDenied("sends are closed".into()))
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn a_recorded_send_is_answered_even_after_the_policy_changes() {
    let toggle = Arc::new(Toggle::default());
    let policy = toggle.clone();
    let hive = Hive::start(move |services| services.with_send_policy(policy), |seat, _| seat).await;
    let send = |call: &str| json!({"agent_id": "b", "message_id": call, "body": "hi", "turn": hive.turn_id(), "call": call});
    let first = hive.a.call("hivemind_send_agent", send("s1")).await;
    assert!(!first.1, "{first:?}");
    toggle.0.store(true, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(hive.a.call("hivemind_send_agent", send("s1")).await, first);
    let (text, failed) = hive.a.call("hivemind_send_agent", send("s2")).await;
    assert!(failed && text.contains("sends are closed"), "{text}");
}
