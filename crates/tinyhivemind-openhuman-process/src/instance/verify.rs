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

/// The recovery tool 0.64.15 adds to a non-empty allowlist while tool
/// output can be compacted or summarized.
const RECOVERY: &str = "juice_retrieve";
/// The tools it adds while results are stored behind a handle.
const REPL: [&str; 3] = ["juice_find", "juice_extract", "juice_summarize"];
/// The tool whose presence in an allowlist adds every deferred tool.
const TOOL_SEARCH: &str = "tool_search";
/// Stand-ins for tools only a running session knows the names of. None is
/// ever expected, so any of them fails the check.
const DEFERRED: &str = "<deferred tools behind tool_search>";
const DELEGATION: &str = "<the session's delegation tools>";

fn enabled(value: &Value) -> bool {
    // A switch the snapshot leaves out is the binary's default, on.
    value != false
}

/// The tools the model sees, computed as 0.64.15's `resolved_scope` does
/// (`agent/tinyagents/host/definition_registry.rs`): the chat agent's
/// allowlist, of the tools the instance has; every deferred tool if it names
/// `tool_search`; the session's delegation tools for an agent with
/// subagents; the recovery tool while compaction or the orchestrator's
/// summary stage is on, and the REPL tools while results are stored behind a
/// handle, both only for a non-empty allowlist; and `extra_tools`. No RPC
/// reports the set, so the live tests check it at the model.
pub(super) fn visible(config: &Value, available: &[String]) -> Vec<String> {
    let agent = config["agent"]["chat_agent_id"]
        .as_str()
        .unwrap_or("orchestrator");
    let entries = config["agent_registry"]["entries"].as_array().cloned();
    let Some(entry) = entries
        .unwrap_or_default()
        .into_iter()
        .find(|entry| entry["id"] == agent)
    else {
        return vec![format!("<the built-in agent {agent}>")];
    };
    let has = |name: &str| available.iter().any(|tool| tool == name);
    let list = |key: &str| {
        let names = entry[key].as_array().cloned().unwrap_or_default();
        names
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let allowed = list("tool_allowlist");
    let mut seen: Vec<String> = allowed.iter().filter(|name| has(name)).cloned().collect();
    if allowed.iter().any(|name| name == TOOL_SEARCH) {
        seen.push(DEFERRED.to_owned());
    }
    if !list("subagents").is_empty() {
        seen.push(DELEGATION.to_owned());
    }
    let compaction = enabled(&config["context"]["compaction_enabled"]);
    let threshold = &config["context"]["summarizer_payload_threshold_tokens"];
    let summarizes = agent == "orchestrator" && threshold.as_u64() != Some(0);
    let juice = &config["tokenjuice"];
    let repl = compaction
        && enabled(&juice["router_enabled"])
        && enabled(&juice["ccr_enabled"])
        && enabled(&juice["repl_handle_enabled"]);
    if !allowed.is_empty() {
        let mut companions = Vec::new();
        if compaction || summarizes {
            companions.push(RECOVERY);
        }
        if repl {
            companions.extend(REPL);
        }
        seen.extend(
            companions
                .into_iter()
                .filter(|name| has(name))
                .map(str::to_owned),
        );
    }
    seen.extend(list("extra_tools"));
    let mut unique = Vec::new();
    for tool in seen {
        if !unique.contains(&tool) {
            unique.push(tool);
        }
    }
    unique
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
