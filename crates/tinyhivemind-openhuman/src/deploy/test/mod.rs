//! Deployment construction and policy regression tests.
use crate::config::{HiveConfig, Validate as _};
use crate::deploy::{EnvironmentSecrets, HiveDeployment};
#[tokio::test]
async fn refuses_unavailable_host_session_storage_before_starting_runtime() -> anyhow::Result<()> {
    let config = serde_json::from_str::<HiveConfig>(r#"{"runtime":{"session_store":"host"}}"#)?
        .validate()?;
    let result = Box::pin(HiveDeployment::build(
        config,
        std::sync::Arc::new(EnvironmentSecrets),
    ))
    .await;
    assert!(result.is_err_and(|error| error.to_string().contains("session_store")));
    Ok(())
}

mod construction;
mod permission;
mod support;

mod preflight;

mod factory;
mod turns;

mod workflow;

mod cancellation;

mod delegation;
mod mcp;
mod packages;
mod sends;

mod hook_composition;
mod profile;
mod timeout;
