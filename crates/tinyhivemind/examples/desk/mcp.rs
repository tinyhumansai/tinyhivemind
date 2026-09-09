//! Serving the room's tools over MCP, and collecting what a seat called.
//!
//! Everything about *what* the tools are and *what* a call means lives in
//! [`tinyhivemind::speech`]: the four specs, their descriptions, the validation,
//! and the fold from an accepted call to a row. This module is the transport
//! under that — a minimal MCP server the agent CLI spawns, a JSON-RPC loop, and
//! a per-turn outbox file — and it decides nothing.
//!
//! ## The host still owns storage
//!
//! This server never writes the transcript. It appends to a per-turn **outbox**
//! that the host drains after the process exits, so sequence assignment,
//! audience resolution, mention resolution and dispatch all stay where they
//! were. A tool call is a request to speak; the host is still what decides what
//! that means.

use std::{
    fs::{self, OpenOptions},
    io::{BufRead, Write},
    path::{Path, PathBuf},
};

use tinyhivemind::{
    aside::Audience,
    dispatch::DispatchConversation,
    speech::{
        CallArguments, CommitRequest, ParameterKind, ToolCall, ToolSpec, Utterance,
        addressed_peers, check_recipients, commit_utterance, interpret, tool_specs,
    },
};

use crate::{aside, deskfile, log, room};

/// Where the server looks up what a call means: the desk it is serving, the
/// transcript behind it, and whose turn is running.
///
/// The server is a separate process from the desk loop, so it is handed paths
/// rather than state. Everything it needs is on disk already, and reading it
/// per call costs one small file: a seat calls a tool a handful of times in a
/// turn that runs for minutes.
#[derive(Clone, Debug)]
pub(crate) struct Serving {
    /// Where a turn's calls to the room are collected.
    pub(crate) outbox: PathBuf,
    /// The JSONL transcript `read` pages and an aside is priced against.
    pub(crate) transcript: PathBuf,
    /// The desk file naming the seats.
    pub(crate) desk: Option<PathBuf>,
    /// The file naming the seat whose turn is running.
    pub(crate) turn: Option<PathBuf>,
}

/// Say whose turn is about to run, so the server can price what it says.
///
/// Written beside the outbox and truncated with it. A server that cannot read
/// it answers a `dm` without checking the aside policy, which is the same
/// answer it gave before this existed.
pub(crate) fn open_turn(path: &Path, seat_id: &str) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, seat_id);
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
    text.lines()
        .filter_map(|line| serde_json::from_str::<Utterance>(line).ok())
        .filter(|utterance| !utterance.message().trim().is_empty())
        .collect()
}

/// The MCP block to merge into the agent CLI's configuration.
///
/// The server is this same binary, re-executed. Nothing about it varies per
/// turn — the outbox is one file the host truncates before each turn — so the
/// configuration is built once.
pub(crate) fn config_block(exe: &Path, serving: &Serving, existing: Option<&str>) -> String {
    let mut config = existing
        .and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    if !config.is_object() {
        config = serde_json::json!({});
    }
    let mut command: Vec<String> = vec![
        exe.to_string_lossy().into_owned(),
        "--mcp-server".into(),
        "--outbox".into(),
        serving.outbox.to_string_lossy().into_owned(),
        "--transcript".into(),
        serving.transcript.to_string_lossy().into_owned(),
    ];
    // Both are what let a `desk_dm` be priced against the aside policy inside
    // the turn that made it, rather than after the process has gone.
    if let Some(desk) = &serving.desk {
        command.push("--desk".into());
        command.push(desk.to_string_lossy().into_owned());
    }
    if let Some(turn) = &serving.turn {
        command.push("--turn".into());
        command.push(turn.to_string_lossy().into_owned());
    }
    let block = serde_json::json!({
        "type": "local",
        "enabled": true,
        "command": command,
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
pub(crate) fn serve(serving: &Serving) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
            "tools/call" => match call(&request, serving) {
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

/// The library's tool surface, rendered as MCP tool descriptors.
///
/// Nothing here names a tool, writes a description, or decides a bound. It
/// walks [`tool_specs`] and translates each parameter's shape into JSON Schema.
fn tools() -> serde_json::Value {
    serde_json::Value::Array(tool_specs().iter().map(descriptor).collect())
}

fn descriptor(spec: &ToolSpec) -> serde_json::Value {
    let mut properties = serde_json::Map::new();
    let mut required: Vec<serde_json::Value> = Vec::new();
    for parameter in spec.parameters {
        let mut schema = match parameter.kind {
            ParameterKind::Text => serde_json::json!({ "type": "string" }),
            ParameterKind::TextList => {
                serde_json::json!({ "type": "array", "items": { "type": "string" } })
            }
            ParameterKind::Count { .. } => serde_json::json!({ "type": "integer" }),
        };
        if let Some(description) = parameter.description
            && let Some(object) = schema.as_object_mut()
        {
            object.insert("description".into(), description.into());
        }
        properties.insert(parameter.name.to_string(), schema);
        if parameter.required {
            required.push(parameter.name.into());
        }
    }
    serde_json::json!({
        "name": spec.name,
        "description": spec.description,
        "inputSchema": {
            "type": "object",
            "properties": serde_json::Value::Object(properties),
            "required": serde_json::Value::Array(required),
        },
    })
}

fn call(request: &serde_json::Value, serving: &Serving) -> Result<String, String> {
    let name = request
        .pointer("/params/name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let arguments = request
        .pointer("/params/arguments")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let to = string_list(&arguments, "to");
    let interpreted = interpret(
        name,
        &CallArguments {
            message: arguments.get("message").and_then(serde_json::Value::as_str),
            to: &to,
            limit: arguments.get("limit").and_then(serde_json::Value::as_u64),
        },
    )
    // The refusal is the seat's to read, so it travels as the sentence the
    // library wrote for it rather than as a code this server invents.
    .map_err(|rejection| rejection.to_string())?;
    match interpreted {
        ToolCall::Read { limit } => Ok(recent(&serving.transcript, limit)),
        ToolCall::Speak(utterance) => {
            let priced = price(&utterance, serving)?;
            let acknowledgement = match (&utterance, priced) {
                (Utterance::Post { .. }, _) => "posted to the desk".to_string(),
                (Utterance::Close { .. }, _) => {
                    "posted to the desk; the desk will close after this turn".to_string()
                }
                (Utterance::Dm { to, .. }, None) => format!("sent to @{}", to.join(", @")),
                (Utterance::Dm { .. }, Some(reason)) => format!(
                    "your aside was refused ({reason}) and the message goes to the whole desk \
                     instead; say it as you would in the open, or post it and move on"
                ),
            };
            append(&serving.outbox, &utterance)?;
            Ok(acknowledgement)
        }
    }
}

/// What the room will do with this utterance, before the turn ends.
///
/// Returns the reason a requested aside will be declined, or `None` when it
/// will be honored — and refuses outright, before anything is appended, the one
/// rejection a seat can act on and is most likely to trip: naming somebody who
/// is not on the desk.
///
/// A server with no desk file to read prices nothing and says so by answering
/// `None`. That is the behavior this host had before the check existed: the
/// host still resolves the audience when it drains the outbox, so the row is
/// never wrong — only the seat is uninformed.
fn price(utterance: &Utterance, serving: &Serving) -> Result<Option<String>, String> {
    if !matches!(utterance, Utterance::Dm { .. }) {
        return Ok(None);
    }
    let (Some(desk_path), Some(turn_path)) = (&serving.desk, &serving.turn) else {
        return Ok(None);
    };
    let Ok(text) = fs::read_to_string(desk_path) else {
        return Ok(None);
    };
    let Ok(spec) = deskfile::parse(&text) else {
        return Ok(None);
    };
    let Ok(seat_id) = fs::read_to_string(turn_path) else {
        return Ok(None);
    };
    let seat_id = seat_id.trim();
    if seat_id.is_empty() {
        return Ok(None);
    }
    let room = room::Room::new(&spec);
    let roster = room.roster();
    let desks = room.desks();
    check_recipients(utterance, seat_id, &roster).map_err(|rejection| rejection.to_string())?;

    let rows = log::JsonlLog::open(&serving.transcript)
        .map(|log| log.rows())
        .unwrap_or_default();
    let peers = addressed_peers(utterance, seat_id, &roster, &desks);
    let committed = commit_utterance(&CommitRequest {
        utterance,
        speaker_id: seat_id,
        conversation: &DispatchConversation {
            desk_id: spec.id.clone(),
            thread_root: None,
        },
        aside: aside::ASIDES,
        spent: aside::spent_in_aside(&rows),
        unsettled: aside::unsettled_aside(&rows, seat_id, &peers),
        roster: &roster,
        desks: &desks,
    })
    .map_err(|error| error.to_string())?;
    Ok(match committed.audience {
        Audience::Aside { .. } => None,
        Audience::Desk => Some(
            committed
                .refusal
                .map_or_else(|| "no audience".to_string(), |reason| format!("{reason:?}")),
        ),
    })
}

/// The `to` argument as owned strings, before the library normalizes it.
fn string_list(arguments: &serde_json::Value, key: &str) -> Vec<String> {
    arguments
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.as_str())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn append(outbox: &Path, utterance: &Utterance) -> Result<(), String> {
    let entry = serde_json::to_string(utterance)
        .map_err(|error| format!("the desk outbox is unavailable: {error}"))?;
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
