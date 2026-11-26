//! The host's send policy and secrets reach the calls the server dispatches.
#![allow(clippy::unwrap_used, clippy::expect_used)]
mod support;

use serde_json::json;
use std::sync::Arc;
use support::Hive;
use tinyhivemind_exec::EnvSecrets;
use tinyhivemind_host::policy::{SendAuthorizer, SendRequest};

/// Refuses every direct message.
struct NoDirect;
impl SendAuthorizer for NoDirect {
    fn authorize(&self, _: &str, request: &SendRequest) -> tinyhivemind_host::Result<()> {
        match request {
            SendRequest::Agent { .. } => Err(tinyhivemind_host::Error::SendDenied(
                "no direct messages".into(),
            )),
            _ => Ok(()),
        }
    }
}

#[tokio::test]
async fn the_send_policy_refuses_before_anything_is_sent() {
    let hive = Hive::start(
        |services| {
            let services = services
                .with_send_policy(Arc::new(NoDirect))
                .with_secrets(Arc::new(EnvSecrets));
            assert!(format!("{services:?}").contains("send_policy: true"));
            services
        },
        |seat, _| seat,
    )
    .await;
    let send = json!({"agent_id": "b", "message_id": "m1", "body": "hi", "turn": hive.turn_id(), "call": "s1"});
    let (text, failed) = hive.a.call("hivemind_send_agent", send).await;
    assert!(failed && text.contains("no direct messages"), "{text}");
    let delivered = hive.coordinator.read_direct("a", "b", None).unwrap();
    assert_eq!(delivered.len(), 0);
    assert_eq!(
        hive.server.url(),
        format!("http://{}/mcp", hive.server.local_addr())
    );
}
