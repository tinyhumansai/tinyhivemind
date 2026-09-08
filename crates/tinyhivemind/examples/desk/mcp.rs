//! The room as a tool, not as a fence.
//!
//! A seat's free text is its *thinking*. What it says to the room is an
//! action it takes, and this module is the surface it takes it through: a
//! minimal MCP server the agent CLI spawns and calls, exposing three tools —
//! `post`, `dm`, and `read`.
//!
//! The reason is a defect. A model-authored delimiter is an interface with a
//! fallible producer: run 26 lost a verified result because a seat closed with
//! `<<<POST>>> … <<<POST>>>` instead of `<<<POST … POST>>>` and the room
//! received the three characters `>>>`. A tool call has a schema, the CLI
//! validates it, and a malformed one is rejected *to the seat*, which can then
//! try again inside its own turn rather than losing it.
//!
//! It also gives agent-to-agent communication a shape. Before this, everything
//! a seat said went to one shared context and was distinguished only by the
//! text it happened to contain. Now the address is a field.
//!
//! ## The host still owns storage
//!
//! This server never writes the transcript. It appends to a per-turn **outbox**
//! that the host drains after the process exits, so sequence assignment,
//! audience resolution through [`aside`](tinyhivemind::aside::aside), mention
//! resolution and dispatch all stay where they were. A tool call is a request
//! to speak; the host is still what decides what that means.

use std::{
    fs::{self, OpenOptions},
    io::{BufRead, Write},
    path::Path,
};

/// What a seat asked the room for during one turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Utterance {
    /// A message for the whole desk.
    Post {
        /// The text to append.
        message: String,
    },
    /// A message for named peers only.
    Dm {
        /// The peers addressed.
        to: Vec<String>,
        /// The text to append.
        message: String,
    },
}

impl Utterance {
    /// The text of the message, whoever it is for.
    pub(crate) fn message(&self) -> &str {
        match self {
            Self::Post { message } | Self::Dm { message, .. } => message,
        }
    }
}

/// Empty the outbox before a turn, so a turn only ever drains its own calls.
pub(crate) fn clear_outbox(path: &Path) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, "");
}

/// Read what a turn said, in the order it said it.
///
/// A malformed line is skipped rather than failing the drain: the outbox is
/// written by another process, and a turn that spoke twice and garbled once
/// still spoke.
pub(crate) fn drain_outbox(path: &Path) -> Vec<Utterance> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines().filter_map(parse_utterance).collect()
}

fn parse_utterance(line: &str) -> Option<Utterance> {
    let value = serde_json::from_str::<serde_json::Value>(line).ok()?;
    let message = value.get("message")?.as_str()?.trim().to_string();
    if message.is_empty() {
        return None;
    }
    match value.get("kind")?.as_str()? {
        "post" => Some(Utterance::Post { message }),
        "dm" => {
            let to: Vec<String> = value
                .get("to")?
                .as_array()?
                .iter()
                .filter_map(|entry| entry.as_str())
                .map(|entry| entry.trim_start_matches('@').to_string())
                .filter(|entry| !entry.is_empty())
                .collect();
            (!to.is_empty()).then_some(Utterance::Dm { to, message })
        }
        _ => None,
    }
}

/// The MCP block to merge into the agent CLI's configuration.
///
/// The server is this same binary, re-executed. Nothing about it varies per
/// turn — the outbox is one file the host truncates before each turn — so the
/// configuration is built once.
pub(crate) fn config_block(
    exe: &Path,
    outbox: &Path,
    transcript: &Path,
    existing: Option<&str>,
) -> String {
    let mut config = existing
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if !config.is_object() {
        config = serde_json::json!({});
    }
    let block = serde_json::json!({
        "type": "local",
        "enabled": true,
        "command": [
            exe.to_string_lossy(),
            "--mcp-server",
            "--outbox",
            outbox.to_string_lossy(),
            "--transcript",
            transcript.to_string_lossy(),
        ],
    });
    let object = config.as_object_mut().unwrap_or_else(|| unreachable!());
    let servers = object.entry("mcp").or_insert_with(|| serde_json::json!({}));
    if let Some(servers) = servers.as_object_mut() {
        servers.insert("desk".into(), block);
    }
    config.to_string()
}

/// Serve MCP over stdio until the caller closes it.
///
/// # Errors
///
/// Returns a write failure on stdout. A malformed request is answered with a
/// JSON-RPC error rather than ending the server.
pub(crate) fn serve(
    outbox: &Path,
    transcript: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(request) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        // A notification carries no id and takes no response.
        let Some(id) = request.get("id").cloned() else {
            continue;
        };
        let method = request
            .get("method")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let response = match method {
            "initialize" => ok(&id, &initialize()),
            "ping" => ok(&id, &serde_json::json!({})),
            "tools/list" => ok(&id, &serde_json::json!({ "tools": tools() })),
            "tools/call" => match call(&request, outbox, transcript) {
                Ok(text) => ok(
                    &id,
                    &serde_json::json!({ "content": [{ "type": "text", "text": text }] }),
                ),
                Err(message) => ok(
                    &id,
                    &serde_json::json!({
                        "isError": true,
                        "content": [{ "type": "text", "text": message }],
                    }),
                ),
            },
            other => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": { "code": -32601, "message": format!("unknown method {other}") },
            }),
        };
        writeln!(stdout, "{response}")?;
        stdout.flush()?;
    }
    Ok(())
}

fn ok(id: &serde_json::Value, result: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn initialize() -> serde_json::Value {
    serde_json::json!({
        "protocolVersion": "2024-11-05",
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "desk", "version": env!("CARGO_PKG_VERSION") },
    })
}

fn tools() -> serde_json::Value {
    serde_json::json!([
        {
            "name": "post",
            "description":
                "Say one thing to the whole desk. This is the only way to speak: text you \
                 write outside a tool call is your own thinking and reaches nobody. Mention a \
                 teammate with @id to hand them the next turn — only the first mention does \
                 that. Call this exactly once, at the end of your turn.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description":
                            "What you established, what you did not finish, and the one seat \
                             you need next — that seat named first.",
                    },
                },
                "required": ["message"],
            },
        },
        {
            "name": "dm",
            "description":
                "Say one thing to named peers instead of the whole desk. Use it to settle a \
                 disagreement without spending the room's attention; the room is told the \
                 exchange happened and not what it said. It still costs your one message for \
                 the turn.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "to": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Seat ids, without the @.",
                    },
                    "message": { "type": "string" },
                },
                "required": ["to", "message"],
            },
        },
        {
            "name": "read",
            "description":
                "Read the desk's recent messages. You are handed a bounded window at the top \
                 of your turn; call this when you need more of it than you were given.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "How many recent messages to return. Default 20, max 100.",
                    },
                },
            },
        },
    ])
}

fn call(request: &serde_json::Value, outbox: &Path, transcript: &Path) -> Result<String, String> {
    let name = request
        .pointer("/params/name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let arguments = request
        .pointer("/params/arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    match name {
        "post" => {
            let message = text_argument(&arguments, "message")?;
            append(
                outbox,
                &serde_json::json!({ "kind": "post", "message": message }),
            )?;
            Ok("posted to the desk".into())
        }
        "dm" => {
            let message = text_argument(&arguments, "message")?;
            let to: Vec<String> = arguments
                .get("to")
                .and_then(serde_json::Value::as_array)
                .map(|entries| {
                    entries
                        .iter()
                        .filter_map(|entry| entry.as_str())
                        .map(|entry| entry.trim_start_matches('@').to_string())
                        .filter(|entry| !entry.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            if to.is_empty() {
                return Err("`to` must name at least one seat".into());
            }
            append(
                outbox,
                &serde_json::json!({ "kind": "dm", "to": to, "message": message }),
            )?;
            Ok(format!("sent to @{}", to.join(", @")))
        }
        "read" => {
            let limit = arguments
                .get("limit")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(20)
                .clamp(1, 100) as usize;
            Ok(recent(transcript, limit))
        }
        other => Err(format!("unknown tool {other}")),
    }
}

fn text_argument(arguments: &serde_json::Value, key: &str) -> Result<String, String> {
    let value = arguments
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if value.is_empty() {
        return Err(format!("`{key}` must be a non-empty string"));
    }
    Ok(value)
}

fn append(outbox: &Path, entry: &serde_json::Value) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(outbox)
        .map_err(|error| format!("the desk outbox is unavailable: {error}"))?;
    writeln!(file, "{entry}").map_err(|error| format!("the desk outbox is unavailable: {error}"))
}

/// The tail of the transcript, rendered the way a turn's prompt renders it.
fn recent(transcript: &Path, limit: usize) -> String {
    let Ok(text) = fs::read_to_string(transcript) else {
        return "(the desk has no messages yet)".into();
    };
    let mut rendered: Vec<String> = Vec::new();
    for line in text.lines().rev() {
        if rendered.len() >= limit {
            break;
        }
        let Ok(row) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        // A private row is not readable through this tool. The host projects
        // what a viewer may see; this server knows no viewer, so it shows only
        // what every member may read.
        if row
            .pointer("/audience/kind")
            .and_then(serde_json::Value::as_str)
            != Some("desk")
        {
            continue;
        }
        let who = row
            .pointer("/author/id")
            .and_then(serde_json::Value::as_str)
            .or_else(|| {
                row.pointer("/author/label")
                    .and_then(serde_json::Value::as_str)
            })
            .unwrap_or("someone");
        let sequence = row
            .get("sequence")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default();
        let content = row
            .get("content")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        rendered.push(format!("[{sequence}] {who}: {content}"));
    }
    if rendered.is_empty() {
        return "(the desk has no messages yet)".into();
    }
    rendered.reverse();
    rendered.join("\n\n")
}

#[cfg(test)]
mod test;
