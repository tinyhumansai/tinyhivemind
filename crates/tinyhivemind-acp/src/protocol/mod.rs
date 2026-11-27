//! The few Agent Client Protocol messages a seat needs, built and read as
//! JSON.
//!
//! Protocol version 1. The client offers no file system and no terminal, so
//! the agent can only use the tools it is given. Reading is tolerant: a
//! field this crate does not know is ignored, and the usage an agent reports
//! (still unstable in the protocol's schema) is read where the measured
//! agents put it.
use serde_json::{Value, json};
use tinyhivemind_host::harness::{ToolEndpoint, TurnUsage};

/// The protocol version this client speaks.
pub(crate) const VERSION: u64 = 1;

/// `initialize`'s parameters: version 1, no file system, no terminal.
pub(crate) fn initialize() -> Value {
    json!({
        "protocolVersion": VERSION,
        "clientCapabilities": {
            "fs": {"readTextFile": false, "writeTextFile": false},
            "terminal": false,
        },
        "clientInfo": {"name": "tinyhivemind-acp", "version": env!("CARGO_PKG_VERSION")},
    })
}

/// The version an `initialize` result reports for the agent, if any.
pub(crate) fn agent_version(result: &Value) -> Option<&str> {
    result["agentInfo"]["version"].as_str()
}

/// What an agent's `initialize` result promises that a seat relies on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Accepted {
    /// Whether it can `session/load` a session it ran before.
    pub(crate) load_session: bool,
}

/// Accept an `initialize` result, or say why a seat cannot use the agent:
/// it must speak protocol version 1 and, when the host's tools reach it per
/// session (`session_mcp`), connect to an HTTP MCP server. An agent given
/// them elsewhere, as `OpenClaw` is through its Gateway, need not.
pub(crate) fn accept(result: &Value, session_mcp: bool) -> Result<Accepted, String> {
    let version = &result["protocolVersion"];
    if version.as_u64() != Some(VERSION) {
        return Err(format!(
            "the agent speaks protocol version {version}, not {VERSION}"
        ));
    }
    let capabilities = &result["agentCapabilities"];
    if session_mcp && capabilities["mcpCapabilities"]["http"] != true {
        return Err("the agent cannot connect to an HTTP MCP server".to_owned());
    }
    Ok(Accepted {
        load_session: capabilities["loadSession"] == true,
    })
}

/// The host's tool server as an ACP HTTP MCP server, its bearer in a header.
pub(crate) fn mcp_server(tools: &ToolEndpoint) -> Value {
    json!({
        "type": "http",
        "name": tools.server,
        "url": tools.url,
        "headers": [{"name": "Authorization", "value": format!("Bearer {}", tools.bearer())}],
    })
}

/// A prompt of one text block.
pub(crate) fn prompt(session: &str, text: &str) -> Value {
    json!({"sessionId": session, "prompt": [{"type": "text", "text": text}]})
}

/// The usage of a `session/prompt` result: `inputTokens` there excludes the
/// cached reads and writes, which [`TurnUsage::input_tokens`] includes.
pub(crate) fn usage(result: &Value) -> Option<TurnUsage> {
    let usage = result.get("usage").filter(|usage| usage.is_object())?;
    let count = |name: &str| usage[name].as_u64().unwrap_or(0);
    Some(TurnUsage {
        input_tokens: count("inputTokens") + count("cachedReadTokens") + count("cachedWriteTokens"),
        cached_input_tokens: count("cachedReadTokens"),
        output_tokens: count("outputTokens"),
        reasoning_tokens: count("thoughtTokens"),
    })
}

/// Why a turn that stopped for `reason` failed, or `None` if it ended
/// normally.
pub(crate) fn failure(reason: &str) -> Option<String> {
    (reason != "end_turn").then(|| format!("the agent stopped: {reason}"))
}

/// One `session/update` this crate acts on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Update {
    /// Some of the agent's reply.
    Text(String),
    /// A tool call started, with the MCP server it goes to when the agent
    /// says.
    ToolCall {
        id: String,
        title: String,
        server: Option<String>,
    },
    /// A tool call ended.
    ToolEnded { id: String, failed: bool },
    /// Anything else.
    Other,
}

/// Read a `session/update` notification's `update`.
pub(crate) fn update(update: &Value) -> Update {
    let text = |name: &str| update[name].as_str().unwrap_or_default().to_owned();
    match update["sessionUpdate"].as_str() {
        Some("agent_message_chunk") => match update["content"]["text"].as_str() {
            Some(chunk) => Update::Text(chunk.to_owned()),
            None => Update::Other,
        },
        Some("tool_call") => Update::ToolCall {
            id: text("toolCallId"),
            title: text("title"),
            server: update["rawInput"]["server"].as_str().map(str::to_owned),
        },
        Some("tool_call_update") => match update["status"].as_str() {
            Some(status @ ("completed" | "failed")) => Update::ToolEnded {
                id: text("toolCallId"),
                failed: status == "failed",
            },
            _ => Update::Other,
        },
        _ => Update::Other,
    }
}

/// The option of a `session/request_permission` that allows the call once,
/// or failing that at all.
pub(crate) fn allow(options: &Value) -> Option<String> {
    pick(options, &["allow_once", "allow_always"])
}

/// The answer that refuses the call: its reject option, or cancelled.
pub(crate) fn refuse(options: &Value) -> Value {
    match pick(options, &["reject_once", "reject_always"]) {
        Some(option) => selected(&option),
        None => cancelled(),
    }
}

/// The answer to a permission request whose turn was cancelled.
pub(crate) fn cancelled() -> Value {
    json!({"outcome": {"outcome": "cancelled"}})
}

/// The answer that selects `option`.
pub(crate) fn selected(option: &str) -> Value {
    json!({"outcome": {"outcome": "selected", "optionId": option}})
}

fn pick(options: &Value, kinds: &[&str]) -> Option<String> {
    let options = options.as_array()?;
    kinds.iter().find_map(|kind| {
        options
            .iter()
            .find(|option| option["kind"] == *kind)
            .and_then(|option| option["optionId"].as_str())
            .map(str::to_owned)
    })
}

#[cfg(test)]
mod test;
