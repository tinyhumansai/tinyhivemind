//! `Belt::authorize` runs the policy a call would meet, and nothing else.
use super::support::{coordinator, with_agents};
use crate::{
    Error, Result,
    policy::{
        AgentCreator, CreateFuture, Management, ManagementAuthorizer, ManagementRequest, NewAgent,
        SendAuthorizer, SendRequest,
    },
    tools::{Belt, Kind, ToolCall},
};
use serde_json::json;

/// Refuses every direct message.
struct NoDirect;
impl SendAuthorizer for NoDirect {
    fn authorize(&self, _: &str, request: &SendRequest) -> Result<()> {
        match request {
            SendRequest::Agent { .. } => Err(Error::SendDenied("no direct messages".into())),
            _ => Ok(()),
        }
    }
}
/// Refuses joins; builds nothing.
struct NoJoins;
impl ManagementAuthorizer for NoJoins {
    fn authorize(&self, _: &str, request: &ManagementRequest) -> Result<()> {
        match request {
            ManagementRequest::JoinHive { .. } => Err(Error::Unauthorized("no joins".into())),
            _ => Ok(()),
        }
    }
}
impl AgentCreator for NoJoins {
    fn create(&self, _: NewAgent) -> CreateFuture<'_> {
        Box::pin(async { Err(Error::Host("creates nothing".into())) })
    }
}

#[tokio::test]
async fn authorize_refuses_what_the_send_policy_refuses_and_sends_nothing() -> Result<()> {
    let coordinator = with_agents(&["a", "b"]).await?;
    let belt = Belt::new(&coordinator).with_send_policy(&NoDirect);
    let direct = json!({"agent_id": "b", "message_id": "m1", "body": "hi"});
    let call = ToolCall::new("a", None, Kind::SendAgent, direct)?;
    assert!(matches!(belt.authorize(&call), Err(Error::SendDenied(_))));
    assert_eq!(coordinator.read_direct("a", "b", None)?.len(), 0);
    let post = json!({"episode_id": "e", "body": "b"});
    belt.authorize(&ToolCall::new("a", None, Kind::Post, post)?)?;
    belt.authorize(&ToolCall::new("a", None, Kind::ListHives, json!({}))?)?;
    Ok(())
}

#[tokio::test]
async fn authorize_refuses_what_management_refuses_or_lacks() -> Result<()> {
    let coordinator = coordinator().await?;
    let join = json!({"hive_id": "h", "agent_id": "a"});
    let call = ToolCall::new("a", None, Kind::JoinHive, join)?;
    assert!(matches!(
        Belt::new(&coordinator).authorize(&call),
        Err(Error::ManagementDisabled)
    ));
    let managed = Belt::new(&coordinator).with_management(Management {
        authorizer: &NoJoins,
        creator: &NoJoins,
    });
    assert!(matches!(
        managed.authorize(&call),
        Err(Error::Unauthorized(_))
    ));
    let leave = json!({"hive_id": "h", "agent_id": "a"});
    managed.authorize(&ToolCall::new("a", None, Kind::LeaveHive, leave)?)?;
    Ok(())
}
