//! Request attribution and failure mapping.
// Test assertions deliberately panic on invalid fixture construction.
#![allow(clippy::unwrap_used)]
use super::*;
use openhuman_embed::{AgentSpec, Runtime, Workspace};
use std::{future::Future, pin::Pin};

struct StalledContext;

impl TurnHooks for StalledContext {
    fn turn_timeout(&self, _: &TurnScope) -> Option<Duration> {
        Some(Duration::from_millis(10))
    }

    fn context_async<'a>(
        &'a self,
        _: &'a TurnScope,
    ) -> Pin<Box<dyn Future<Output = String> + Send + 'a>> {
        Box::pin(std::future::pending())
    }
}

#[test]
fn async_context_loading_uses_the_turn_deadline() {
    let _lock = crate::RUNTIME_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(16 * 1024 * 1024)
        .build()
        .unwrap()
        // Run on a worker thread: the runtime build future outgrows the test
        // thread's default stack, and only the workers get the larger one.
        .block_on(async {
            tokio::spawn(async {
                let backend = crate::offline::backend().await;
                let runtime = Box::pin(
                    Runtime::builder()
                        .config(crate::offline::config())
                        .workspace(Workspace::Ephemeral)
                        .backend_url(backend.uri())
                        .build(),
                )
                .await
                .unwrap();
                let agent = runtime.agent(AgentSpec::new("stalled-context")).unwrap();
                let activation = Arc::new(Activation::default());
                activation.activate();
                let runner = SuppliedRunner {
                    agent: Arc::new(tokio::sync::RwLock::new(Some(agent))),
                    hooks: Arc::new(StalledContext),
                    activation,
                    timeout: Duration::from_secs(1),
                    source: None,
                };
                let request = TurnRequest {
                    turn_id: "turn".into(),
                    scheduled_job_id: None,
                    teammates: Vec::new(),
                    agent_id: "stalled-context".into(),
                    session_id: None,
                    messages: Vec::new(),
                    memberships: Vec::new(),
                    episode: None,
                    resumption: None,
                };
                let result = runner.run(request).await;
                assert!(result.is_err_and(|error| error.to_string().contains("timed out")));
            })
            .await
            .unwrap()
        });
}
#[test]
fn renders_attribution_and_explicit_episode_without_reseeding() {
    let request = TurnRequest {
        turn_id: String::new(),
        scheduled_job_id: None,
        teammates: Vec::new(),
        agent_id: "a".into(),
        session_id: Some("history".into()),
        messages: vec![],
        memberships: vec![],
        episode: None,
        resumption: None,
    };
    let prompt = render(&request).unwrap();
    assert!(prompt.contains("\"agent_id\":\"a\""));
    assert!(prompt.contains("\"session_id\":\"history\""));
    assert!(
        map_error(&Error::TimedOut)
            .to_string()
            .contains("timed out")
    );
    assert!(!prompt.contains("Host resumption note"));
}
#[test]
fn renders_a_release_note_ahead_of_the_attributed_context() {
    let request = TurnRequest {
        turn_id: String::new(),
        scheduled_job_id: None,
        teammates: Vec::new(),
        agent_id: "a".into(),
        session_id: None,
        messages: vec![],
        memberships: vec![],
        episode: None,
        resumption: Some("approved: staging only".into()),
    };
    let prompt = render(&request).unwrap();
    let note = prompt
        .find("Host resumption note: approved: staging only")
        .unwrap();
    assert!(note < prompt.find("Incoming attributed Hivemind context").unwrap());
}
