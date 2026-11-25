//! The host send policy gates the four outbound tools before they act.
use super::support::{call, hive, with_agents};
use crate::{
    Error, Result,
    policy::{SendAuthorizer, SendRequest},
    tools::{Belt, Kind, ToolCall},
};
use serde_json::json;
use std::sync::{Mutex, PoisonError};

type Seen = (String, Option<String>, SendRequest);
/// Refuses any body mentioning "secret"; records every request it sees.
#[derive(Default)]
struct NoSecrets(Mutex<Vec<Seen>>);
impl NoSecrets {
    fn seen(&self) -> Vec<Seen> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}
impl SendAuthorizer for NoSecrets {
    fn authorize(&self, actor: &str, request: &SendRequest) -> Result<()> {
        self.authorize_in_session(actor, request, None)
    }
    fn authorize_in_session(
        &self,
        actor: &str,
        request: &SendRequest,
        session: Option<&str>,
    ) -> Result<()> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).push((
            actor.into(),
            session.map(str::to_owned),
            request.clone(),
        ));
        let body = match request {
            SendRequest::Agent { body, .. }
            | SendRequest::Hive { body, .. }
            | SendRequest::Ask { body, .. }
            | SendRequest::Broadcast { body, .. } => body,
        };
        if body.contains("secret") {
            Err(Error::SendDenied("no secrets leave this agent".into()))
        } else {
            Ok(())
        }
    }
}

#[tokio::test]
async fn refused_sends_carry_the_policy_text_and_enqueue_nothing() -> Result<()> {
    let coordinator = with_agents(&["a", "b"]).await?;
    coordinator.create_hive(hive(&["a", "b"])).await?;
    let policy = NoSecrets::default();
    let belt = Belt::new(&coordinator).with_send_policy(&policy);
    let cases = [
        (
            Kind::SendAgent,
            json!({"agent_id":"b","message_id":"m1","body":"the secret"}),
            SendRequest::Agent {
                agent_id: "b".into(),
                body: "the secret".into(),
            },
        ),
        (
            Kind::SendHive,
            json!({"hive_id":"h","message_id":"m2","body":"secret plan","only_for":["b"]}),
            SendRequest::Hive {
                hive_id: "h".into(),
                body: "secret plan".into(),
                thread: None,
                only_for: vec!["b".into()],
            },
        ),
        (
            Kind::Ask,
            json!({"episode_id":"e","agents":["b"],"body":"secret?"}),
            SendRequest::Ask {
                episode_id: "e".into(),
                agents: vec!["b".into()],
                body: "secret?".into(),
            },
        ),
        (
            Kind::Broadcast,
            json!({"episode_id":"e","body":"secret work"}),
            SendRequest::Broadcast {
                episode_id: "e".into(),
                body: "secret work".into(),
            },
        ),
    ];
    for (kind, args, request) in cases {
        let refused = belt
            .call(&ToolCall::new("a", Some("s".into()), kind, args)?)
            .await;
        assert!(
            refused.is_err_and(
                |error| error.to_string() == "send denied: no secrets leave this agent"
            )
        );
        assert_eq!(
            policy.seen().pop(),
            Some(("a".into(), Some("s".into()), request))
        );
    }
    assert_eq!(coordinator.read_transcript(None)?.len(), 0);
    Ok(())
}

#[tokio::test]
async fn admitted_sends_act_and_other_tools_are_not_consulted() -> Result<()> {
    let coordinator = with_agents(&["a", "b"]).await?;
    let policy = NoSecrets::default();
    let belt = Belt::new(&coordinator).with_send_policy(&policy);
    call(
        belt,
        "a",
        Kind::SendAgent,
        json!({"agent_id":"b","message_id":"ok","body":"hello"}),
    )
    .await?;
    assert_eq!(coordinator.read_transcript(None)?.len(), 1);
    for (kind, args) in [
        (Kind::Post, json!({"episode_id":"e","body":"secret"})),
        (Kind::Complete, json!({"episode_id":"e","body":"secret"})),
    ] {
        assert!(matches!(
            call(belt, "a", kind, args).await,
            Err(Error::Coordinator(_))
        ));
    }
    call(belt, "a", Kind::ListAgents, json!({})).await?;
    assert_eq!(policy.seen().len(), 1);
    Ok(())
}

#[test]
fn a_policy_that_ignores_sessions_decides_by_actor_alone() {
    struct Closed;
    impl SendAuthorizer for Closed {
        fn authorize(&self, actor: &str, _: &SendRequest) -> Result<()> {
            Err(Error::SendDenied(actor.into()))
        }
    }
    let request = SendRequest::Broadcast {
        episode_id: "e".into(),
        body: "b".into(),
    };
    let refused = Closed.authorize_in_session("a", &request, Some("s"));
    assert!(matches!(refused, Err(Error::SendDenied(actor)) if actor == "a"));
}
