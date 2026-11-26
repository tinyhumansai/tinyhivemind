//! A call identified by its turn acts once, through the coordinator's record.
use super::support::{coordinator, hive};
use crate::{
    Error, Result,
    policy::{
        AgentCreator, CreateFuture, Management, ManagementAuthorizer, ManagementRequest, NewAgent,
    },
    tools::{Belt, Kind, ToolCall},
};
use serde_json::{Value, json};
use std::sync::Arc;
use tinyhivemind_hives::{
    AgentRegistration, AgentRunner, Coordinator, Destination, HiveInfo, SendMessage,
    TurnDisposition, TurnFuture, TurnOutcome, TurnRequest,
};
use tokio::sync::{Notify, mpsc};

/// Hands its first turn's request to the test and holds that turn open.
struct Holds {
    started: mpsc::UnboundedSender<TurnRequest>,
    finish: Arc<Notify>,
}
impl AgentRunner for Holds {
    fn run(&self, request: TurnRequest) -> TurnFuture {
        let (started, finish) = (self.started.clone(), self.finish.clone());
        Box::pin(async move {
            if started.send(request.clone()).is_ok() {
                finish.notified().await;
            }
            Ok(TurnOutcome {
                session_id: "stable".into(),
                reply: None,
                disposition: TurnDisposition::Completed,
            })
        })
    }
}
/// Admits everything; creates nothing.
struct Open;
impl ManagementAuthorizer for Open {
    fn authorize(&self, _: &str, _: &ManagementRequest) -> Result<()> {
        Ok(())
    }
}
impl AgentCreator for Open {
    fn create(&self, _: NewAgent) -> CreateFuture<'_> {
        Box::pin(async { Err(Error::Host("creates nothing".into())) })
    }
}

/// `a`'s running turn in hive `h`, with `b` registered beside it.
async fn running() -> Result<(Coordinator, TurnRequest, Arc<Notify>)> {
    let coordinator = coordinator().await?;
    let (started, mut requests) = mpsc::unbounded_channel();
    let finish = Arc::new(Notify::new());
    for id in ["a", "b"] {
        coordinator
            .register_agent(AgentRegistration {
                agent_id: id.into(),
                runtime_id: "r".into(),
                runner: Arc::new(Holds {
                    started: started.clone(),
                    finish: finish.clone(),
                }),
            })
            .await?;
    }
    coordinator.create_hive(hive(&["a"])).await?;
    coordinator
        .send_as_host(SendMessage {
            message_id: "start".into(),
            sender: String::new(),
            destination: Destination::Hive("h".into()),
            body: "work".into(),
            thread: None,
            only_for: vec![],
            starters: Vec::new(),
        })
        .await?;
    tokio::spawn({
        let coordinator = coordinator.clone();
        async move { coordinator.run_until_idle().await }
    });
    let request = requests.recv().await.ok_or(Error::ManagementDisabled)?;
    Ok((coordinator, request, finish))
}
fn identified(request: &TurnRequest, call: &str, kind: Kind, args: Value) -> Result<ToolCall> {
    Ok(ToolCall::new(&request.agent_id, None, kind, args)?.in_turn(&request.turn_id, call))
}
fn episode(request: &TurnRequest) -> String {
    request
        .episode
        .as_ref()
        .map(|episode| episode.episode_id.clone())
        .unwrap_or_default()
}

#[tokio::test]
async fn an_identified_post_retried_acts_once_and_another_call_acts_again() -> Result<()> {
    let (coordinator, request, finish) = running().await?;
    let belt = Belt::new(&coordinator);
    let args = json!({"episode_id": episode(&request), "body": "once"});
    let first = identified(&request, "p1", Kind::Post, args.clone())?;
    assert_eq!(belt.call(&first).await?, json!({"accepted": true}));
    assert_eq!(belt.call(&first).await?, json!({"accepted": true}));
    let second = identified(&request, "p2", Kind::Post, args)?;
    belt.call(&second).await?;
    let reused = identified(
        &request,
        "p1",
        Kind::Post,
        json!({"episode_id": episode(&request), "body": "changed"}),
    )?;
    assert!(matches!(
        belt.call(&reused).await,
        Err(Error::Coordinator(tinyhivemind_hives::Error::CallReused(_)))
    ));
    finish.notify_one();
    Ok(())
}

#[tokio::test]
async fn an_identified_call_naming_another_turn_is_refused() -> Result<()> {
    let (coordinator, request, finish) = running().await?;
    let belt = Belt::new(&coordinator);
    let args = json!({"episode_id": episode(&request), "body": "late"});
    let stale = ToolCall::new("a", None, Kind::Post, args)?.in_turn("0:0:a", "p1");
    assert!(matches!(
        belt.call(&stale).await,
        Err(Error::Coordinator(
            tinyhivemind_hives::Error::StaleTurn { .. }
        ))
    ));
    finish.notify_one();
    Ok(())
}

#[tokio::test]
async fn an_identified_send_retried_returns_the_first_receipt() -> Result<()> {
    let (coordinator, request, finish) = running().await?;
    let belt = Belt::new(&coordinator);
    let args = json!({"agent_id": "b", "message_id": "m1", "body": "hi"});
    let send = identified(&request, "s1", Kind::SendAgent, args)?;
    let receipt = belt.call(&send).await?;
    assert_eq!(belt.call(&send).await?, receipt);
    let delivered = coordinator.read_direct("a", "b", None)?;
    assert_eq!(delivered.len(), 1);
    finish.notify_one();
    Ok(())
}

#[tokio::test]
async fn identified_hive_management_acts_once() -> Result<()> {
    let (coordinator, request, finish) = running().await?;
    let belt = Belt::new(&coordinator).with_management(Management {
        authorizer: &Open,
        creator: &Open,
    });
    let create = identified(
        &request,
        "c1",
        Kind::CreateHive,
        json!({"hive_id": "side", "name": "Side"}),
    )?;
    belt.call(&create).await?;
    let join = identified(
        &request,
        "j1",
        Kind::JoinHive,
        json!({"hive_id": "side", "agent_id": "b"}),
    )?;
    assert_eq!(belt.call(&join).await?, json!({"joined": true}));
    let leave = identified(
        &request,
        "l1",
        Kind::LeaveHive,
        json!({"hive_id": "side", "agent_id": "b"}),
    )?;
    assert_eq!(belt.call(&leave).await?, json!({"left": true}));
    // A retried join is answered from its record and joins nobody.
    assert_eq!(belt.call(&join).await?, json!({"joined": true}));
    let side: Vec<HiveInfo> = coordinator
        .list_hives()?
        .into_iter()
        .filter(|hive| hive.hive_id == "side")
        .collect();
    assert_eq!(side[0].members, Vec::<String>::new());
    finish.notify_one();
    Ok(())
}

/// Admits sends until closed, counting what it is asked.
#[derive(Default)]
struct Closing {
    closed: std::sync::atomic::AtomicBool,
    asked: std::sync::atomic::AtomicUsize,
}
impl crate::policy::SendAuthorizer for Closing {
    fn authorize(&self, _: &str, _: &crate::policy::SendRequest) -> Result<()> {
        use std::sync::atomic::Ordering;
        self.asked.fetch_add(1, Ordering::SeqCst);
        if self.closed.load(Ordering::SeqCst) {
            Err(Error::SendDenied("closed".into()))
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn a_recorded_send_is_answered_without_asking_the_policy_again() -> Result<()> {
    use std::sync::atomic::Ordering;
    let (coordinator, request, finish) = running().await?;
    let policy = Closing::default();
    let belt = Belt::new(&coordinator).with_send_policy(&policy);
    let args = json!({"agent_id": "b", "message_id": "m1", "body": "hi"});
    let send = identified(&request, "s1", Kind::SendAgent, args)?;
    let receipt = belt.call(&send).await?;
    policy.closed.store(true, Ordering::SeqCst);
    assert_eq!(belt.call(&send).await?, receipt);
    assert_eq!(policy.asked.load(Ordering::SeqCst), 1);
    let args = json!({"agent_id": "b", "message_id": "m2", "body": "hi"});
    let other = identified(&request, "s2", Kind::SendAgent, args)?;
    assert!(matches!(belt.call(&other).await, Err(Error::SendDenied(_))));
    finish.notify_one();
    Ok(())
}
