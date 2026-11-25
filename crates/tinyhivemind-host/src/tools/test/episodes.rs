//! Episode actions count only while the caller holds the named assignment.
use super::support::{call, coordinator, hive, with_agents};
use crate::{
    Error, Result,
    tools::{Belt, Kind},
};
use serde_json::json;
use std::sync::{Arc, Mutex, PoisonError};
use tinyhivemind_hives::{
    AgentRegistration, AgentRunner, Coordinator, Destination, SendMessage, TurnDisposition,
    TurnFuture, TurnOutcome, TurnRequest,
};

/// Posts and completes through the belt during its own turn.
struct Acts {
    coordinator: Arc<Mutex<Option<Coordinator>>>,
    results: Arc<Mutex<Vec<String>>>,
}
impl AgentRunner for Acts {
    fn run(&self, request: TurnRequest) -> TurnFuture {
        let coordinator = self
            .coordinator
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let results = self.results.clone();
        Box::pin(async move {
            if let (Some(coordinator), Some(episode)) = (coordinator, request.episode) {
                for kind in [Kind::Post, Kind::Complete] {
                    let outcome = call(
                        Belt::new(&coordinator),
                        &request.agent_id,
                        kind,
                        json!({"episode_id":episode.episode_id,"body":"native active action"}),
                    )
                    .await;
                    results
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push(outcome.map_or_else(|e| e.to_string(), |v| v.to_string()));
                }
            }
            Ok(TurnOutcome {
                session_id: "stable".into(),
                reply: None,
                disposition: TurnDisposition::Completed,
            })
        })
    }
}

#[tokio::test]
async fn actions_are_accepted_during_the_bound_assignment() -> Result<()> {
    let coordinator = coordinator().await?;
    let slot = Arc::new(Mutex::new(Some(coordinator.clone())));
    let results = Arc::new(Mutex::new(Vec::new()));
    coordinator
        .register_agent(AgentRegistration {
            agent_id: "a".into(),
            runtime_id: "r".into(),
            runner: Arc::new(Acts {
                coordinator: slot.clone(),
                results: results.clone(),
            }),
        })
        .await?;
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
    assert_eq!(coordinator.run_until_idle().await?.completed, 1);
    assert_eq!(
        *results.lock().unwrap_or_else(PoisonError::into_inner),
        [r#"{"accepted":true}"#, r#"{"accepted":true}"#]
    );
    let rows = coordinator.read_hive("a", "h", None, None)?;
    assert!(
        rows.iter()
            .any(|message| message.sender == "a" && message.body == "native active action")
    );
    // The coordinator holds the runner, which holds the coordinator.
    slot.lock().unwrap_or_else(PoisonError::into_inner).take();
    Ok(())
}

#[tokio::test]
async fn a_stale_episode_is_refused_for_every_action() -> Result<()> {
    let coordinator = with_agents(&["a", "b"]).await?;
    let belt = Belt::new(&coordinator);
    for (kind, args) in [
        (Kind::Post, json!({"episode_id":"stale","body":"b"})),
        (
            Kind::Ask,
            json!({"episode_id":"stale","body":"b","agents":["b"]}),
        ),
        (Kind::Broadcast, json!({"episode_id":"stale","body":"b"})),
        (Kind::Complete, json!({"episode_id":"stale","body":"b"})),
    ] {
        assert!(matches!(
            call(belt, "a", kind, args).await,
            Err(Error::Coordinator(_))
        ));
    }
    Ok(())
}
