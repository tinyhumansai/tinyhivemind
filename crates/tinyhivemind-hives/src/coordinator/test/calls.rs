//! Identified calls: one effect per (agent, turn, call), recorded with it.
use super::transactions::Recording;
use super::*;
use crate::{SqliteStorage, Storage};
use serde_json::{Value, json};
use tokio::sync::{Notify, mpsc};

/// A turn held open by its runner until the test lets it return.
struct Held {
    request: TurnRequest,
    finish: Arc<Notify>,
    drain: tokio::task::JoinHandle<Result<RunReport>>,
}
impl Held {
    fn episode(&self) -> String {
        self.request.episode.as_ref().unwrap().episode_id.clone()
    }
    fn call(&self, call: &str, tool: &str, arguments: Value) -> TurnCall {
        TurnCall {
            agent_id: self.request.agent_id.clone(),
            turn_id: self.request.turn_id.clone(),
            call: call.into(),
            tool: tool.into(),
            arguments,
        }
    }
    fn post(&self, call: &str, body: &str) -> TurnCall {
        self.call(
            call,
            "hivemind_post",
            json!({"episode_id": self.episode(), "body": body}),
        )
    }
    async fn end(self) -> RunReport {
        self.finish.notify_one();
        self.drain.await.unwrap().unwrap()
    }
}
/// Register `ids`, each holding its turns open, in a hive `work`.
async fn holding(
    c: &Coordinator,
    ids: &[&str],
) -> (mpsc::UnboundedReceiver<TurnRequest>, Arc<Notify>) {
    let (started, requests) = mpsc::unbounded_channel();
    let finish = Arc::new(Notify::new());
    for id in ids {
        let (started, finish) = (started.clone(), finish.clone());
        add(c, id, move |request| {
            let (started, finish) = (started.clone(), finish.clone());
            Box::pin(async move {
                started.send(request.clone()).unwrap();
                finish.notified().await;
                Ok(done(&request))
            })
        })
        .await;
    }
    hive(c, "work", ids).await;
    (requests, finish)
}
/// Open an episode for `a` and hold its turn.
async fn hold(c: &Coordinator) -> Held {
    let (mut requests, finish) = holding(c, &["a"]).await;
    c.send_as_host(message("task", Destination::Hive("work".into())))
        .await
        .unwrap();
    let drain = tokio::spawn({
        let c = c.clone();
        async move { c.run_until_idle().await }
    });
    let request = requests.recv().await.unwrap();
    Held {
        request,
        finish,
        drain,
    }
}
fn actions(c: &Coordinator, agent: &str) -> Vec<EpisodeAction> {
    c.lock().unwrap().durable.running[agent].actions.clone()
}
fn posted(body: &str) -> EpisodeAction {
    EpisodeAction::Post { body: body.into() }
}

#[tokio::test]
async fn the_running_turn_is_reported_by_id() {
    let c = setup().await;
    assert_eq!(c.running_turn("a").unwrap(), None);
    let held = hold(&c).await;
    assert_eq!(
        c.running_turn("a").unwrap().as_deref(),
        Some(held.request.turn_id.as_str())
    );
    held.end().await;
    assert_eq!(c.running_turn("a").unwrap(), None);
}

#[tokio::test]
async fn a_retried_post_acts_once_and_returns_the_first_result() {
    let c = setup().await;
    let held = hold(&c).await;
    let call = held.post("post-1", "hello");
    let episode = held.episode();
    c.submit_action_in_turn(&call, &episode, posted("hello"))
        .await
        .unwrap();
    let revision = c.lock().unwrap().durable.revision;
    c.submit_action_in_turn(&call, &episode, posted("hello"))
        .await
        .unwrap();
    assert_eq!(actions(&c, "a"), [posted("hello")]);
    // The answer came from the record, so nothing was committed.
    assert_eq!(c.lock().unwrap().durable.revision, revision);
    held.end().await;
}

#[tokio::test]
async fn two_identical_posts_under_different_calls_both_act() {
    let c = setup().await;
    let held = hold(&c).await;
    let episode = held.episode();
    for call in ["post-1", "post-2"] {
        c.submit_action_in_turn(&held.post(call, "hello"), &episode, posted("hello"))
            .await
            .unwrap();
    }
    assert_eq!(actions(&c, "a"), [posted("hello"), posted("hello")]);
    held.end().await;
}

#[tokio::test]
async fn reusing_a_call_with_other_arguments_or_another_tool_is_refused() {
    let c = setup().await;
    let held = hold(&c).await;
    let episode = held.episode();
    c.submit_action_in_turn(&held.post("one", "hello"), &episode, posted("hello"))
        .await
        .unwrap();
    let other_arguments = held.post("one", "goodbye");
    assert!(matches!(
        c.submit_action_in_turn(&other_arguments, &episode, posted("goodbye")).await,
        Err(Error::CallReused(call)) if call == "one"
    ));
    let mut other_tool = held.post("one", "hello");
    other_tool.tool = "hivemind_complete".into();
    let complete = EpisodeAction::Complete {
        body: "hello".into(),
    };
    assert!(matches!(
        c.submit_action_in_turn(&other_tool, &episode, complete)
            .await,
        Err(Error::CallReused(_))
    ));
    assert_eq!(actions(&c, "a"), [posted("hello")]);
    held.end().await;
}

#[tokio::test]
async fn a_call_naming_another_turn_is_refused_whatever_its_call() {
    let c = setup().await;
    let held = hold(&c).await;
    let episode = held.episode();
    let mut stale = held.post("post-1", "hello");
    stale.turn_id = "0:0:a".into();
    assert!(matches!(
        c.submit_action_in_turn(&stale, &episode, posted("hello")).await,
        Err(Error::StaleTurn { agent_id, turn_id }) if agent_id == "a" && turn_id == "0:0:a"
    ));
    assert!(actions(&c, "a").is_empty());
    assert!(c.lock().unwrap().durable.running["a"].calls.is_empty());
    held.end().await;
}

#[tokio::test]
async fn a_call_with_no_running_turn_is_refused() {
    let c = setup().await;
    let held = hold(&c).await;
    let call = held.post("post-1", "hello");
    let episode = held.episode();
    held.end().await;
    assert!(matches!(
        c.submit_action_in_turn(&call, &episode, posted("hello"))
            .await,
        Err(Error::StaleTurn { .. })
    ));
}

#[tokio::test]
async fn a_call_without_an_identity_is_refused() {
    let c = setup().await;
    let held = hold(&c).await;
    let episode = held.episode();
    let blank = held.post("  ", "hello");
    assert!(matches!(
        c.submit_action_in_turn(&blank, &episode, posted("hello"))
            .await,
        Err(Error::InvalidIdentifier("call"))
    ));
    assert!(actions(&c, "a").is_empty());
    held.end().await;
}

#[tokio::test]
async fn another_seats_episode_is_refused_and_records_nothing() {
    let c = setup().await;
    let held = hold(&c).await;
    let call = held.call(
        "post-1",
        "hivemind_post",
        json!({"episode_id": "episode:99", "body": "hello"}),
    );
    assert!(matches!(
        c.submit_action_in_turn(&call, "episode:99", posted("hello"))
            .await,
        Err(Error::StaleEpisode(_))
    ));
    assert!(c.lock().unwrap().durable.running["a"].calls.is_empty());
    held.end().await;
}

#[tokio::test]
async fn a_failed_commit_leaves_neither_the_effect_nor_its_record() {
    let storage = Arc::new(Recording::default());
    let c = Coordinator::new(
        "runtime".into(),
        storage.clone(),
        CoordinatorOptions::default(),
    )
    .await
    .unwrap();
    let held = hold(&c).await;
    let call = held.post("post-1", "hello");
    let episode = held.episode();
    storage.fail_next_commits(1);
    assert!(
        c.submit_action_in_turn(&call, &episode, posted("hello"))
            .await
            .is_err()
    );
    let stored = storage.load().await.unwrap();
    assert!(stored.running["a"].actions.is_empty());
    assert!(stored.running["a"].calls.is_empty());
    assert!(actions(&c, "a").is_empty());
    // The same call then acts, once.
    c.submit_action_in_turn(&call, &episode, posted("hello"))
        .await
        .unwrap();
    c.submit_action_in_turn(&call, &episode, posted("hello"))
        .await
        .unwrap();
    assert_eq!(actions(&c, "a"), [posted("hello")]);
    held.end().await;
}

#[tokio::test]
async fn the_record_survives_a_storage_reload_of_a_running_turn() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("hive.sqlite");
    let storage = Arc::new(SqliteStorage::open(&path).unwrap());
    let c = Coordinator::new("runtime".into(), storage, CoordinatorOptions::default())
        .await
        .unwrap();
    let held = hold(&c).await;
    let call = held.post("post-1", "hello");
    c.submit_action_in_turn(&call, &held.episode(), posted("hello"))
        .await
        .unwrap();
    let reloaded = SqliteStorage::open(&path).unwrap().load().await.unwrap();
    assert_eq!(
        reloaded.running["a"].calls["post-1"],
        CallRecord {
            tool: "hivemind_post".into(),
            arguments: call.arguments.clone(),
            result: Value::Null,
        }
    );
    held.end().await;
}

#[tokio::test]
async fn after_a_coordinator_restart_the_retry_is_refused() {
    let storage = Arc::new(MemoryStorage::new());
    let c = Coordinator::new(
        "runtime".into(),
        storage.clone(),
        CoordinatorOptions::default(),
    )
    .await
    .unwrap();
    let held = hold(&c).await;
    let call = held.post("post-1", "hello");
    let episode = held.episode();
    c.submit_action_in_turn(&call, &episode, posted("hello"))
        .await
        .unwrap();
    let restarted = Coordinator::new("runtime".into(), storage, CoordinatorOptions::default())
        .await
        .unwrap();
    assert!(matches!(
        restarted
            .submit_action_in_turn(&call, &episode, posted("hello"))
            .await,
        Err(Error::StaleTurn { .. })
    ));
    held.drain.abort();
}

#[tokio::test]
async fn the_record_goes_when_its_turn_ends() {
    let storage = Arc::new(MemoryStorage::new());
    let c = Coordinator::new(
        "runtime".into(),
        storage.clone(),
        CoordinatorOptions::default(),
    )
    .await
    .unwrap();
    let held = hold(&c).await;
    let call = held.post("post-1", "hello");
    c.submit_action_in_turn(&call, &held.episode(), posted("hello"))
        .await
        .unwrap();
    held.end().await;
    assert!(storage.load().await.unwrap().running.is_empty());
}

fn direct(id: &str, sender: &str) -> SendMessage {
    SendMessage {
        sender: sender.into(),
        ..message(id, Destination::Agent("b".into()))
    }
}

#[tokio::test]
async fn a_retried_send_acts_once_and_returns_the_first_receipt() {
    let c = setup().await;
    let (mut requests, finish) = holding(&c, &["a"]).await;
    add(&c, "b", |request| {
        Box::pin(async move { Ok(done(&request)) })
    })
    .await;
    c.send_as_host(message("task", Destination::Agent("a".into())))
        .await
        .unwrap();
    let drain = tokio::spawn({
        let c = c.clone();
        async move { c.run_until_idle().await }
    });
    let request = requests.recv().await.unwrap();
    let held = Held {
        request,
        finish,
        drain,
    };
    let call = held.call("send-1", "hivemind_send_agent", json!({"body": "hi"}));
    let first = c.send_in_turn(&call, direct("note", "a")).await.unwrap();
    let again = c.send_in_turn(&call, direct("note", "a")).await.unwrap();
    assert_eq!(first, again);
    // Two calls with one `message_id` and one payload are one message.
    let other = held.call("send-2", "hivemind_send_agent", json!({"body": "hi"}));
    assert_eq!(
        c.send_in_turn(&other, direct("note", "a")).await.unwrap(),
        first
    );
    let notes = c
        .read_transcript(None)
        .unwrap()
        .into_iter()
        .filter(|message| message.message_id == "note")
        .count();
    assert_eq!(notes, 1);
    // A send can only be the caller's own.
    let forged = held.call("send-3", "hivemind_send_agent", json!({"body": "hi"}));
    assert!(matches!(
        c.send_in_turn(&forged, direct("forged", "b")).await,
        Err(Error::InvalidIdentifier("sender"))
    ));
    held.end().await;
}

#[tokio::test]
async fn a_retried_join_acts_once_and_returns_the_first_result() {
    let c = setup().await;
    let held = hold(&c).await;
    c.create_hive(HiveInfo {
        hive_id: "side".into(),
        name: "Side".into(),
        description: None,
        members: Vec::new(),
    })
    .await
    .unwrap();
    let call = held.call(
        "join-1",
        "hivemind_join_hive",
        json!({"hive_id": "side", "agent_id": "a"}),
    );
    c.join_hive_in_turn(&call, "side", "a").await.unwrap();
    c.leave_hive("side", "a").await.unwrap();
    c.join_hive_in_turn(&call, "side", "a").await.unwrap();
    let side = c
        .list_hives()
        .unwrap()
        .into_iter()
        .find(|h| h.hive_id == "side");
    assert!(side.unwrap().members.is_empty());
    held.end().await;
}

#[tokio::test]
async fn hive_management_in_a_turn_acts_and_is_recorded() {
    let c = setup().await;
    let held = hold(&c).await;
    let hive = HiveInfo {
        hive_id: "side".into(),
        name: "Side".into(),
        description: None,
        members: vec!["a".into()],
    };
    let create = held.call(
        "create-1",
        "hivemind_create_hive",
        json!({"hive_id": "side"}),
    );
    c.create_hive_in_turn(&create, hive.clone()).await.unwrap();
    let leave = held.call(
        "leave-1",
        "hivemind_leave_hive",
        json!({"hive_id": "side", "agent_id": "a"}),
    );
    c.leave_hive_in_turn(&leave, "side", "a").await.unwrap();
    c.join_hive("side", "a").await.unwrap();
    // Neither retry acts again: the hive is not recreated and `a` stays.
    c.create_hive_in_turn(&create, hive).await.unwrap();
    c.leave_hive_in_turn(&leave, "side", "a").await.unwrap();
    let side = c
        .list_hives()
        .unwrap()
        .into_iter()
        .find(|h| h.hive_id == "side");
    assert_eq!(side.unwrap().members, ["a"]);
    let calls = c.lock().unwrap().durable.running["a"].calls.clone();
    assert_eq!(calls.keys().collect::<Vec<_>>(), ["create-1", "leave-1"]);
    held.end().await;
}
