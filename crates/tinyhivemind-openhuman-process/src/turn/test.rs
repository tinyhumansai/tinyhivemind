//! One turn over `channel_web_chat` and `/events`, against the fake surface.
use super::{Active, Ended, Turn, cancel, run};
use crate::fixture::Fixture;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, PoisonError};
use tinyhivemind_host::harness::{
    ApprovalFuture, Approver, DenyAll, Event, Observer, Progress, ToolApproval, ToolDecision,
    TurnUsage,
};
use tokio::sync::watch;

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

impl Recorder {
    fn progress(&self) -> Vec<Progress> {
        let events = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let progress = events.iter().filter_map(|event| match event {
            Event::Progress { progress, .. } => Some(progress.clone()),
            _ => None,
        });
        progress.collect()
    }
}

struct AllowAll;

impl Approver for AllowAll {
    fn decide(&self, _: ToolApproval) -> ApprovalFuture {
        Box::pin(async { ToolDecision::Allow })
    }
}

struct Harnessed {
    fixture: Fixture,
    events: Arc<Recorder>,
    active: Arc<Mutex<Option<Active>>>,
    closed: watch::Sender<bool>,
}

impl Harnessed {
    async fn new(steps: &[Value]) -> Self {
        let fixture = Fixture::new().await;
        fixture.script(steps);
        Self {
            fixture,
            events: Arc::default(),
            active: Arc::default(),
            closed: watch::channel(false).0,
        }
    }

    fn turn<'a>(&'a self, approver: &'a dyn Approver, iterations: usize) -> Turn<'a> {
        Turn {
            client: &self.fixture.client,
            seat: "scout",
            thread: "thm-scout",
            id: "t1",
            client_id: "thm-1".into(),
            iterations,
            observer: self.events.as_ref(),
            approver,
            approval_limit: std::time::Duration::from_secs(5),
            closed: self.closed.subscribe(),
        }
    }

    async fn run(&self, iterations: usize) -> super::Result<Ended> {
        run(self.turn(&DenyAll, iterations), "go", &self.active).await
    }
}

#[tokio::test]
async fn a_turn_ends_with_its_reply_and_its_own_usage() {
    let harnessed = Harnessed::new(&[json!({"reply": "done", "tokens": 40})]).await;
    let ended = harnessed.run(8).await;
    let usage = TurnUsage {
        input_tokens: 40,
        cached_input_tokens: 4,
        output_tokens: 2,
        reasoning_tokens: 0,
    };
    let expected = Ended {
        reply: Some("done".into()),
        usage: Some(usage),
        failure: None,
    };
    assert_eq!(ended.ok(), Some(expected));
    // Published before `channel_web_chat` answered, and still seen.
    let progress = harnessed.events.progress();
    assert_eq!(
        progress.first(),
        Some(&Progress::ModelCall { iteration: 1 })
    );
    assert!(progress.contains(&Progress::Text {
        delta: "done".into()
    }));
    assert!(harnessed.active.lock().is_ok_and(|active| active.is_none()));
}

#[tokio::test]
async fn another_requests_events_are_not_this_turns() {
    let step = json!({"reply": "mine", "tokens": 1, "stray": true});
    let harnessed = Harnessed::new(&[step]).await;
    let ended = harnessed.run(8).await;
    assert_eq!(
        ended.ok().and_then(|ended| ended.reply),
        Some("mine".into())
    );
}

#[tokio::test]
async fn a_chat_error_fails_the_turn() {
    let harnessed = Harnessed::new(&[json!({"error": "provider down"})]).await;
    let ended = harnessed.run(8).await;
    let failure = ended.ok().and_then(|ended| ended.failure);
    assert!(failure.is_some_and(|failure| failure.contains("provider down")));
}

#[tokio::test]
async fn reaching_the_eight_iteration_cap_is_a_failed_turn_with_its_usage() {
    let step = json!({"reply": "checkpoint", "tokens": 9, "iterations": 8});
    let harnessed = Harnessed::new(&[step]).await;
    let ended = harnessed.run(8).await.ok();
    let failure = ended.as_ref().and_then(|ended| ended.failure.clone());
    assert!(failure.is_some_and(|failure| failure.contains("cap of 8")));
    let usage = ended.and_then(|ended| ended.usage);
    assert_eq!(usage.map(|usage| usage.input_tokens), Some(9));
}

#[tokio::test]
async fn a_seat_limit_below_the_cap_fails_the_turn_past_it() {
    let step = json!({"reply": "late", "tokens": 1, "iterations": 5});
    let harnessed = Harnessed::new(&[step]).await;
    let ended = harnessed.run(3).await.ok();
    let failure = ended.and_then(|ended| ended.failure);
    assert!(failure.is_some_and(|failure| failure.contains("limit of 3")));
    let seven = Harnessed::new(&[json!({"reply": "ok", "iterations": 3})]).await;
    let ended = seven.run(3).await.ok();
    assert_eq!(ended.and_then(|ended| ended.failure), None);
}

#[tokio::test]
async fn a_harness_approval_is_decided_by_the_approver() {
    let step = json!({"reply": "after", "approval": "shell"});
    let harnessed = Harnessed::new(std::slice::from_ref(&step)).await;
    assert!(harnessed.run(8).await.is_ok());
    let allowed = Harnessed::new(&[step]).await;
    assert!(
        run(allowed.turn(&AllowAll, 8), "go", &allowed.active)
            .await
            .is_ok()
    );
    let decided = |harnessed: &Harnessed| {
        let log = harnessed.fixture.log("decisions.jsonl");
        log.iter()
            .map(|entry| entry["decision"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(decided(&harnessed), [json!("deny")]);
    assert_eq!(decided(&allowed), [json!("approve_once")]);
}

#[tokio::test]
async fn a_tool_call_and_its_result_are_progress() {
    let step = json!({"reply": "ok", "tool": "mcp_hivemind_hivemind_post_000001"});
    let harnessed = Harnessed::new(&[step]).await;
    assert!(harnessed.run(8).await.is_ok());
    let progress = harnessed.events.progress();
    let call = Progress::ToolCall {
        call_id: "call_1".into(),
        tool: "mcp_hivemind_hivemind_post_000001".into(),
    };
    let result = Progress::ToolResult {
        call_id: "call_1".into(),
        failed: true,
    };
    assert!(
        progress.contains(&call) && progress.contains(&result),
        "{progress:?}"
    );
}

#[tokio::test]
async fn a_cancelled_turn_is_shown_to_have_stopped() {
    let harnessed = Arc::new(Harnessed::new(&[Value::Null]).await);
    let running = harnessed.clone();
    let task = tokio::spawn(async move { running.run(8).await.is_ok() });
    while harnessed.fixture.calls() != ["open"] {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    task.abort();
    let active = harnessed
        .active
        .lock()
        .ok()
        .and_then(|mut active| active.take());
    let active = active.unwrap_or_else(|| unreachable!("a dropped turn leaves itself"));
    assert!(active.request_id.is_some());
    let cancelled = cancel(&harnessed.fixture.client, &active).await;
    assert!(cancelled.is_ok(), "{cancelled:?}");
    assert_eq!(harnessed.fixture.calls(), ["aborted"]);
    // Nothing is running any more, so a second cancel returns at once.
    let again = cancel(&harnessed.fixture.client, &active).await;
    assert!(again.is_ok());
}

#[tokio::test]
async fn a_closed_turn_ends_without_waiting_for_the_harness() {
    let harnessed = Arc::new(Harnessed::new(&[Value::Null]).await);
    let running = harnessed.clone();
    let task = tokio::spawn(async move { running.run(8).await.err().map(|e| e.to_string()) });
    while harnessed.fixture.calls() != ["open"] {
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let _ = harnessed.closed.send(true);
    let ended = task.await.ok().flatten();
    assert!(ended.is_some_and(|error| error.contains("closed")));
}

#[tokio::test]
async fn a_message_the_instance_does_not_accept_fails_and_agent_chat_is_never_used() {
    let fixture = Fixture::with(&[]).await;
    let harnessed = Harnessed {
        fixture,
        events: Arc::default(),
        active: Arc::default(),
        closed: watch::channel(false).0,
    };
    let mut turn = harnessed.turn(&DenyAll, 8);
    turn.thread = "";
    let ended = run(turn, "go", &harnessed.active).await;
    assert!(ended.is_err());
    let methods = harnessed.fixture.log("methods.jsonl");
    assert!(!methods.contains(&json!("openhuman.inference_agent_chat")));
    assert!(methods.contains(&json!("openhuman.channel_web_chat")));
}

/// Never answers.
struct Silent;

impl Approver for Silent {
    fn decide(&self, _: ToolApproval) -> ApprovalFuture {
        Box::pin(std::future::pending())
    }
}

#[tokio::test]
async fn an_approver_that_does_not_answer_in_time_is_a_denial() {
    let harnessed = Harnessed::new(&[json!({"reply": "after", "approval": "shell"})]).await;
    let mut turn = harnessed.turn(&Silent, 8);
    turn.approval_limit = std::time::Duration::from_millis(100);
    assert!(run(turn, "go", &harnessed.active).await.is_ok());
    let decided = harnessed.fixture.log("decisions.jsonl");
    assert_eq!(
        decided.first().map(|entry| entry["decision"].clone()),
        Some(json!("deny"))
    );
}

#[tokio::test]
async fn an_approval_request_without_an_id_fails_the_turn_clearly() {
    let step = json!({"reply": "after", "approval": "shell", "approval_id": ""});
    let harnessed = Harnessed::new(&[step]).await;
    let ended = harnessed.run(8).await;
    let error = ended.err().map(|error| error.to_string());
    assert!(error.is_some_and(|error| error.contains("approval_request without a request_id")));
}

#[tokio::test]
async fn an_approval_left_by_a_dropped_turn_is_denied_before_the_next() {
    let harnessed =
        Arc::new(Harnessed::new(&[json!({"reply": "late", "approval": "shell"})]).await);
    let running = harnessed.clone();
    let task = tokio::spawn(async move {
        run(running.turn(&Silent, 8), "go", &running.active)
            .await
            .is_ok()
    });
    let client = harnessed.fixture.client.clone();
    let mut pending = 0;
    while pending == 0 {
        let listed = client
            .call("openhuman.approval_list_pending", json!({}))
            .await;
        pending = listed
            .ok()
            .and_then(|listed| listed["result"].as_array().map(Vec::len))
            .unwrap_or(0);
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    task.abort();
    let denied = super::deny_stale(&client).await;
    assert_eq!(denied.ok(), Some(1));
    let decided = harnessed.fixture.log("decisions.jsonl");
    assert_eq!(
        decided.first().map(|entry| entry["decision"].clone()),
        Some(json!("deny"))
    );
    assert_eq!(super::deny_stale(&client).await.ok(), Some(0));
}

#[tokio::test]
async fn a_chat_error_is_bounded_and_carries_no_payload() {
    let payload = format!("upstream said:\n{{\"error\": \"{}\"}}", "x".repeat(5000));
    let harnessed = Harnessed::new(&[json!({"error": payload})]).await;
    let failure = harnessed.run(8).await.ok().and_then(|ended| ended.failure);
    let failure = failure.unwrap_or_else(|| unreachable!("a failure"));
    assert!(failure.len() <= 300, "{} bytes", failure.len());
    assert!(!failure.contains('\n'), "{failure}");
    assert!(failure.starts_with("chat_error (inference)"), "{failure}");
}
