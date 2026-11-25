//! Management is authorized before any mutation; only creation needs a harness.
use super::support::{Reply, call, with_agents};
use crate::{
    Error, Result,
    policy::{
        AgentCreator, CreateFuture, Management, ManagementAuthorizer, ManagementRequest, NewAgent,
    },
    tools::{Belt, Kind},
};
use serde_json::json;
use std::sync::{Arc, Mutex, PoisonError};
use tinyhivemind_hives::{AgentRegistration, Coordinator};

/// Admits `a` alone.
struct OnlyA;
impl ManagementAuthorizer for OnlyA {
    fn authorize(&self, actor: &str, _: &ManagementRequest) -> Result<()> {
        if actor == "a" {
            Ok(())
        } else {
            Err(Error::Unauthorized("actor".into()))
        }
    }
}
/// Registers what it is asked to create, and records it.
struct Creator<'a> {
    coordinator: &'a Coordinator,
    asked: Mutex<Vec<NewAgent>>,
}
impl AgentCreator for Creator<'_> {
    fn create(&self, agent: NewAgent) -> CreateFuture<'_> {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(agent.clone());
        Box::pin(async move {
            if agent.template == "missing" {
                return Err(Error::Host("no such template".into()));
            }
            let agent_id = agent.coordinator_id.unwrap_or_else(|| "built".into());
            self.coordinator
                .register_agent(AgentRegistration {
                    agent_id: agent_id.clone(),
                    runtime_id: "r".into(),
                    runner: Arc::new(Reply),
                })
                .await?;
            Ok(agent_id)
        })
    }
}

#[tokio::test]
async fn an_authorized_actor_manages_hives_and_agents() -> Result<()> {
    let coordinator = with_agents(&["a", "b"]).await?;
    let creator = Creator {
        coordinator: &coordinator,
        asked: Mutex::default(),
    };
    let belt = Belt::new(&coordinator).with_management(Management {
        authorizer: &OnlyA,
        creator: &creator,
    });
    let created = call(
        belt,
        "a",
        Kind::CreateHive,
        json!({"hive_id":"h","name":"Hive","members":["a"],"description":"purpose"}),
    )
    .await?;
    assert_eq!(created["description"], "purpose");
    for (kind, done) in [
        (Kind::JoinHive, json!({"joined":true})),
        (Kind::LeaveHive, json!({"left":true})),
    ] {
        let args = json!({"hive_id":"h","agent_id":"b"});
        assert_eq!(call(belt, "a", kind, args).await?, done);
    }
    let agent = call(
        belt,
        "a",
        Kind::CreateAgent,
        json!({"template":"scout","coordinator_id":"scout-1","config":{"depth":2}}),
    )
    .await?;
    assert_eq!(agent, json!({"agent_id":"scout-1"}));
    assert_eq!(
        *creator.asked.lock().unwrap_or_else(PoisonError::into_inner),
        [NewAgent {
            template: "scout".into(),
            coordinator_id: Some("scout-1".into()),
            config: json!({"depth":2}),
            memory: None,
        }]
    );
    assert_eq!(coordinator.list_agents()?, ["a", "b", "scout-1"]);
    let failed = call(
        belt,
        "a",
        Kind::CreateAgent,
        json!({"template":"missing","config":{}}),
    )
    .await;
    assert!(failed.is_err_and(|error| error.to_string() == "no such template"));
    Ok(())
}

#[tokio::test]
async fn a_refused_actor_changes_nothing_and_reaches_no_creator() -> Result<()> {
    let coordinator = with_agents(&["a", "b"]).await?;
    let creator = Creator {
        coordinator: &coordinator,
        asked: Mutex::default(),
    };
    let belt = Belt::new(&coordinator).with_management(Management {
        authorizer: &OnlyA,
        creator: &creator,
    });
    for (kind, args) in [
        (Kind::CreateHive, json!({"hive_id":"h","name":"Hive"})),
        (Kind::CreateAgent, json!({"template":"scout","config":{}})),
    ] {
        assert!(matches!(
            call(belt, "b", kind, args).await,
            Err(Error::Unauthorized(_))
        ));
    }
    assert!(coordinator.list_hives()?.is_empty());
    assert!(
        creator
            .asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn management_is_disabled_until_the_host_configures_it() -> Result<()> {
    let coordinator = with_agents(&["a"]).await?;
    let belt = Belt::new(&coordinator);
    for (kind, args) in [
        (Kind::CreateHive, json!({"hive_id":"h","name":"Hive"})),
        (Kind::LeaveHive, json!({"hive_id":"h","agent_id":"a"})),
    ] {
        assert!(matches!(
            call(belt, "a", kind, args).await,
            Err(Error::ManagementDisabled)
        ));
    }
    Ok(())
}

#[tokio::test]
async fn an_undecodable_memory_contract_is_refused_before_authorization() -> Result<()> {
    let coordinator = with_agents(&["a"]).await?;
    let refused = call(
        Belt::new(&coordinator),
        "a",
        Kind::CreateAgent,
        json!({"template":"t","config":{},"memory":{"root":1}}),
    )
    .await;
    assert!(matches!(refused, Err(Error::Json(_))));
    Ok(())
}
