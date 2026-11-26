//! A coordinator with two registered agents and nothing to run.
use crate::{
    Result,
    tools::{Belt, Kind, ToolCall},
};
use serde_json::Value;
use std::sync::Arc;
use tinyhivemind_hives::{
    AgentRegistration, AgentRunner, Coordinator, CoordinatorOptions, HiveInfo, MemoryStorage,
    TurnDisposition, TurnFuture, TurnOutcome, TurnRequest,
};

/// Replies to every turn and completes it.
pub(super) struct Reply;
impl AgentRunner for Reply {
    fn run(&self, request: TurnRequest) -> TurnFuture {
        Box::pin(async move {
            Ok(TurnOutcome {
                session_id: format!("session:{}", request.agent_id),
                reply: Some("answer".into()),
                disposition: TurnDisposition::Completed,
            })
        })
    }
}

/// A coordinator with no agents.
pub(super) async fn coordinator() -> Result<Coordinator> {
    Ok(Coordinator::new(
        "r".into(),
        Arc::new(MemoryStorage::new()),
        CoordinatorOptions::default(),
    )
    .await?)
}

/// A coordinator with these agents registered over [`Reply`].
pub(super) async fn with_agents(ids: &[&str]) -> Result<Coordinator> {
    let coordinator = coordinator().await?;
    for id in ids {
        coordinator
            .register_agent(AgentRegistration {
                agent_id: (*id).into(),
                runtime_id: "r".into(),
                runner: Arc::new(Reply),
            })
            .await?;
    }
    Ok(coordinator)
}

/// Hive `h` of `members`.
pub(super) fn hive(members: &[&str]) -> HiveInfo {
    HiveInfo {
        hive_id: "h".into(),
        name: "Hive".into(),
        description: None,
        members: members.iter().map(|id| (*id).into()).collect(),
    }
}

/// One validated call as `actor`.
pub(super) async fn call(belt: Belt<'_>, actor: &str, kind: Kind, args: Value) -> Result<Value> {
    belt.call(&ToolCall::new(actor, None, kind, args)?).await
}
