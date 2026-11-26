//! Whether a started instance is the one that was asked for, and the names
//! its model sees the hive tools under.
use crate::config::{AGENT_ID, Written};
use crate::error::{Error, Result};
use crate::rpc::Client;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::time::Duration;
use tinyhivemind_host::harness::{Expected, Observed, Started};
use tokio::time::Instant;

/// How often a warming instance is asked again.
const POLL: Duration = Duration::from_millis(100);

/// Every tool name the instance offers.
async fn available(client: &Client) -> Result<Vec<String>> {
    let listed = client
        .call("openhuman.agent_registry_available_tools", json!({}))
        .await?;
    let tools = listed["tools"].as_array().cloned().unwrap_or_default();
    let names = tools
        .iter()
        .filter_map(|tool| tool["name"].as_str().map(str::to_owned));
    Ok(names.collect())
}

/// The name `tool` of `server` has among `names`: `mcp_<server>_<tool>_` and
/// a hex hash, which no other tool's name also matches.
fn name_of<'a>(names: &'a [String], server: &str, tool: &str) -> Option<&'a String> {
    let prefix = format!("mcp_{server}_{tool}_");
    let mut found = names.iter().filter(|name| {
        name.strip_prefix(&prefix)
            .is_some_and(|hash| !hash.is_empty() && hash.bytes().all(|b| b.is_ascii_hexdigit()))
    });
    let first = found.next();
    first.filter(|_| found.next().is_none())
}

/// The model-visible names of `tools`, served by `server`, once the instance
/// has connected to it, within `warm_up`.
///
/// # Errors
/// [`Error::ToolsNotListed`] for tools still unnamed when `warm_up` is over.
pub(crate) async fn discover(
    client: &Client,
    server: &str,
    tools: &BTreeSet<String>,
    warm_up: Duration,
) -> Result<Vec<String>> {
    let deadline = Instant::now() + warm_up;
    loop {
        let names = available(client).await?;
        let found: Vec<_> = tools
            .iter()
            .map(|tool| name_of(&names, server, tool))
            .collect();
        if found.iter().all(Option::is_some) {
            let mut found: Vec<String> = found.into_iter().flatten().cloned().collect();
            found.sort();
            return Ok(found);
        }
        if Instant::now() >= deadline {
            let missing = tools.iter().zip(&found).filter(|(_, name)| name.is_none());
            return Err(Error::ToolsNotListed {
                server: server.to_owned(),
                missing: missing.map(|(tool, _)| tool.clone()).collect(),
            });
        }
        tokio::time::sleep(POLL).await;
    }
}

/// The tool 0.64.15 adds to any allowlist while tool-output compaction is
/// on, so that the model can recover a compacted result.
const RECOVERY: &str = "juice_retrieve";

/// The tools the model sees: the agent's effective allowlist, of those the
/// instance has, and the recovery tool unless compaction is off, which the
/// binary defaults it to.
pub(super) fn visible(config: &Value, available: &[String]) -> Vec<String> {
    let entries = config["agent_registry"]["entries"].as_array().cloned();
    let agent = entries
        .unwrap_or_default()
        .into_iter()
        .find(|entry| entry["id"] == AGENT_ID);
    let allowed = agent.map_or(Value::Null, |agent| agent["tool_allowlist"].clone());
    let allowed = allowed.as_array().cloned().unwrap_or_default();
    let names = allowed.iter().filter_map(Value::as_str);
    let mut seen: Vec<String> = names
        .filter(|name| available.iter().any(|tool| tool == name))
        .map(str::to_owned)
        .collect();
    if config["context"]["compaction_enabled"] != false {
        seen.push(RECOVERY.to_owned());
    }
    seen
}

/// Hold the instance at `client` against the child `pid`, the pinned
/// `version`, the `written` config and exactly `tools`, waiting up to
/// `warm_up` for its MCP tools to be listed.
///
/// # Errors
/// [`Error::Mismatch`] for any difference, or the API's own failure.
pub(crate) async fn verify(
    client: &Client,
    pid: u32,
    version: &str,
    written: &Written,
    tools: &[String],
    warm_up: Duration,
) -> Result<Started> {
    let health = client.health().await?;
    let answering = health["pid"]
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok());
    let running = client.call("core.version", json!({})).await?;
    let running = running["version"].as_str().unwrap_or_default().to_owned();
    let snapshot = client.call("openhuman.config_get", json!({})).await?;
    let config = &snapshot["result"]["config"];
    let deadline = Instant::now() + warm_up;
    let seen = loop {
        let seen = visible(config, &available(client).await?);
        let all = tools.iter().all(|tool| seen.contains(tool));
        if all || Instant::now() >= deadline {
            break seen;
        }
        tokio::time::sleep(POLL).await;
    };
    let mut expected = Expected::new(pid, version).tools(tools.iter().cloned());
    for (name, value) in written.settings() {
        expected = expected.setting(name, value);
    }
    let mut observed = Observed::new(answering.unwrap_or_default(), running).tools(seen);
    for (name, value) in written.observe(config) {
        observed = observed.setting(name, value);
    }
    Ok(expected.verify(&observed)?)
}
