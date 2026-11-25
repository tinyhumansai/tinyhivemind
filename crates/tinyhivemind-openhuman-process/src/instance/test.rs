//! Start-up verification and tool discovery, against the fake surface.
use super::verify::{discover, verify};
use crate::error::Error;
use crate::fixture::{Fixture, PID, SERVER, hashed_tools, served};
use std::collections::BTreeSet;
use std::time::Duration;
use tinyhivemind_host::harness::Mismatch;

const WARM: Duration = Duration::from_secs(5);

async fn verified(fixture: &Fixture, pid: u32) -> Result<tinyhivemind_host::harness::Started, Error> {
    let tools = hashed_tools();
    verify(&fixture.client, pid, "0.64.15", &fixture.written, &tools, WARM).await
}

#[tokio::test]
async fn the_instance_that_was_written_is_verified_after_its_warm_up() {
    let fixture = Fixture::with(&[("warm", "3")]).await;
    let started = verified(&fixture, PID).await;
    let started = started.unwrap_or_else(|error| unreachable!("verified: {error}"));
    assert_eq!(started.pid, PID);
    assert_eq!(started.tools, hashed_tools().into_iter().collect::<BTreeSet<_>>());
    assert!(started.settings.contains("agent_registry.entries"));
    assert!(started.settings.contains("api_url"));
}

#[tokio::test]
async fn another_process_answering_is_refused() {
    let fixture = Fixture::new().await;
    let refused = verified(&fixture, PID + 1).await;
    assert!(matches!(refused, Err(Error::Mismatch(Mismatch::Pid { .. }))), "{refused:?}");
}

#[tokio::test]
async fn another_version_is_refused() {
    let fixture = Fixture::with(&[("version", "0.64.14")]).await;
    let refused = verified(&fixture, PID).await;
    assert!(matches!(refused, Err(Error::Mismatch(Mismatch::Version { .. }))), "{refused:?}");
}

#[tokio::test]
async fn a_config_that_fell_back_to_defaults_is_refused() {
    let fixture = Fixture::with(&[("misconfigured", "")]).await;
    let refused = verified(&fixture, PID).await;
    assert!(matches!(refused, Err(Error::Mismatch(Mismatch::Settings { .. }))), "{refused:?}");
}

#[tokio::test]
async fn a_tool_the_model_would_see_besides_the_hive_tools_is_refused() {
    let fixture = Fixture::with(&[("tools", "extra")]).await;
    let refused = verified(&fixture, PID).await;
    assert!(matches!(refused, Err(Error::Mismatch(_))), "{refused:?}");
}

#[tokio::test]
async fn a_hive_tool_the_model_would_not_see_is_refused() {
    let fixture = Fixture::with(&[("tools", "missing"), ("warm", "0")]).await;
    let tools = hashed_tools();
    let refused = verify(&fixture.client, PID, "0.64.15", &fixture.written, &tools, Duration::ZERO).await;
    let Err(Error::Mismatch(Mismatch::Tools { missing, unexpected })) = refused else {
        unreachable!("refused for its tools: {refused:?}")
    };
    assert_eq!(missing, [tools[1].clone()]);
    assert!(unexpected.is_empty());
}

#[tokio::test]
async fn discovery_waits_out_the_warm_up_and_reads_the_hashed_names() {
    let fixture = Fixture::with(&[("warm", "4")]).await;
    let wanted: BTreeSet<String> = served().into_iter().collect();
    let found = discover(&fixture.client, SERVER, &wanted, WARM).await;
    let mut expected = hashed_tools();
    expected.sort();
    assert_eq!(found.ok(), Some(expected));
}

#[tokio::test]
async fn a_tool_never_listed_fails_discovery_by_name() {
    let fixture = Fixture::with(&[("tools", "missing")]).await;
    let wanted: BTreeSet<String> = served().into_iter().collect();
    let found = discover(&fixture.client, SERVER, &wanted, Duration::from_millis(300)).await;
    let Err(Error::ToolsNotListed { missing, .. }) = found else {
        unreachable!("not listed: {found:?}")
    };
    assert_eq!(missing, ["hivemind_ask"]);
}

#[tokio::test]
async fn a_tool_whose_name_another_tools_name_starts_with_is_told_apart() {
    let fixture = Fixture::with(&[("served", r#"["post", "post_x"]"#), ("warm", "0")]).await;
    let wanted: BTreeSet<String> = ["post".to_owned()].into_iter().collect();
    let found = discover(&fixture.client, SERVER, &wanted, WARM).await;
    assert_eq!(found.ok(), Some(vec![crate::fake::hashed(SERVER, "post")]));
}
