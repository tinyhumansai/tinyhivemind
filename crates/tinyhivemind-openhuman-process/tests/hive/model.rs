//! A Chat Completions model on loopback that acts as a hive seat would.
//!
//! Each request carries one seat's turn: the rendered `TurnRequest` is the
//! latest user message. The model reads the turn and episode from it and,
//! counting the tool results that follow, plays three tool calls and a
//! reply: `hivemind_post` with call `post`, the same call again as a retry,
//! then `hivemind_complete` with call `done`. It records what it was offered
//! and what each tool answered, so a test can compare the retry with the
//! first call.
#[expect(
    dead_code,
    reason = "shared with the fake surface; this model reads bodies only"
)]
#[path = "../support/http.rs"]
mod http;

use http::{Reply, Request, serve};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What one model request showed.
#[derive(Clone, Debug)]
pub(crate) struct Seen {
    /// The seat whose turn it is, from the rendered request.
    pub(crate) agent: String,
    /// The turn, from the rendered request.
    pub(crate) turn: String,
    /// The names of the tools offered.
    pub(crate) offered: Vec<String>,
    /// The tool results since the turn's message, in order.
    pub(crate) results: Vec<String>,
}

/// The model, serving until dropped.
pub(crate) struct HiveModel {
    pub(crate) port: u16,
    seen: Arc<Mutex<Vec<Seen>>>,
    task: tokio::task::JoinHandle<()>,
}

impl HiveModel {
    pub(crate) async fn start() -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let serving = seen.clone();
        let task = tokio::spawn(serve(listener, move |request| {
            let seen = serving.clone();
            async move { answer(&seen, &request) }
        }));
        Ok(Self { port, seen, task })
    }

    /// Every request so far.
    pub(crate) fn seen(&self) -> Vec<Seen> {
        lock(&self.seen).clone()
    }
}

impl Drop for HiveModel {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// The rendered turn in `content`: the JSON after the first `{`.
fn turn_of(content: &str) -> Option<Value> {
    let start = content.find('{')?;
    serde_json::from_str(&content[start..]).ok()
}

/// Text of a message's `content`, a string or a list of text parts.
fn text(message: &Value) -> String {
    match &message["content"] {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .filter_map(|part| part["text"].as_str())
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

/// The offered tool whose name contains `suffix`, as the client named it.
fn tool(offered: &[String], suffix: &str) -> Option<String> {
    offered.iter().find(|name| name.contains(suffix)).cloned()
}

fn answer(seen: &Arc<Mutex<Vec<Seen>>>, request: &Request) -> Reply {
    if request.method == "GET" && request.path.ends_with("/models") {
        return Reply::Json(
            200,
            json!({"object": "list", "data": [{"id": "mock", "object": "model"}]}),
        );
    }
    if !request.path.ends_with("/chat/completions") {
        return Reply::Json(404, json!({}));
    }
    let body = request.json();
    let messages = body["messages"].as_array().cloned().unwrap_or_default();
    let offered: Vec<String> = body["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|tool| tool["function"]["name"].as_str().map(str::to_owned))
        .collect();
    let last_user = messages
        .iter()
        .rposition(|message| message["role"] == "user");
    let rendered = last_user
        .and_then(|index| turn_of(&text(&messages[index])))
        .unwrap_or(Value::Null);
    let results: Vec<String> = last_user
        .map(|index| &messages[index + 1..])
        .unwrap_or_default()
        .iter()
        .filter(|message| message["role"] == "tool")
        .map(text)
        .collect();
    let agent = rendered["agent_id"].as_str().unwrap_or_default().to_owned();
    let turn = rendered["turn_id"].as_str().unwrap_or_default().to_owned();
    let episode = rendered["episode"]["episode_id"]
        .as_str()
        .map(str::to_owned);
    lock(seen).push(Seen {
        agent: agent.clone(),
        turn: turn.clone(),
        offered: offered.clone(),
        results: results.clone(),
    });
    let step = match (episode, results.len()) {
        (Some(episode), 0 | 1) => tool(&offered, "hivemind_post").map(|name| {
            let body = format!("{agent} reporting");
            (
                name,
                json!({"episode_id": episode, "body": body, "turn": turn, "call": "post"}),
            )
        }),
        (Some(episode), 2) => tool(&offered, "hivemind_complete").map(|name| {
            let body = format!("{agent} done");
            (
                name,
                json!({"episode_id": episode, "body": body, "turn": turn, "call": "done"}),
            )
        }),
        _ => None,
    };
    let stream = body["stream"] == true;
    match step {
        Some((name, arguments)) => call(&name, &arguments, results.len(), stream),
        None => reply(&format!("{agent} finished"), stream),
    }
}

const BASE: &str = r#"{"id":"c","created":1,"model":"mock"}"#;

fn usage() -> Value {
    json!({"prompt_tokens": 10, "completion_tokens": 1, "total_tokens": 11})
}

fn frames(chunks: Vec<Value>) -> Reply {
    let (sender, receiver) = mpsc::channel(16);
    let base: Value = serde_json::from_str(BASE).unwrap_or(Value::Null);
    let mut out: Vec<String> = chunks
        .into_iter()
        .map(|mut extra| {
            let mut chunk = base.clone();
            chunk["object"] = json!("chat.completion.chunk");
            if let (Some(chunk), Some(extra)) = (chunk.as_object_mut(), extra.as_object_mut()) {
                chunk.append(extra);
            }
            format!("data: {chunk}\n\n")
        })
        .collect();
    out.push("data: [DONE]\n\n".to_owned());
    tokio::spawn(async move {
        for frame in out {
            let _ = sender.send(frame).await;
        }
    });
    Reply::Stream(receiver)
}

/// A tool call, numbered by how many came before it in the turn.
fn call(name: &str, arguments: &Value, index: usize, stream: bool) -> Reply {
    let id = format!("call-{index}");
    let arguments = arguments.to_string();
    if !stream {
        let mut reply: Value = serde_json::from_str(BASE).unwrap_or(Value::Null);
        reply["object"] = json!("chat.completion");
        reply["choices"] = json!([{"index": 0, "finish_reason": "tool_calls",
            "message": {"role": "assistant", "content": null, "tool_calls": [
                {"id": id, "type": "function", "function": {"name": name, "arguments": arguments}}]}}]);
        reply["usage"] = usage();
        return Reply::Json(200, reply);
    }
    frames(vec![
        json!({"choices": [{"index": 0, "delta": {"role": "assistant", "tool_calls": [
            {"index": 0, "id": id, "type": "function",
             "function": {"name": name, "arguments": arguments}}]}, "finish_reason": null}]}),
        json!({"choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]}),
        json!({"choices": [], "usage": usage()}),
    ])
}

/// A plain answer that ends the turn.
fn reply(text: &str, stream: bool) -> Reply {
    if !stream {
        let mut reply: Value = serde_json::from_str(BASE).unwrap_or(Value::Null);
        reply["object"] = json!("chat.completion");
        reply["choices"] = json!([{"index": 0, "finish_reason": "stop",
            "message": {"role": "assistant", "content": text}}]);
        reply["usage"] = usage();
        return Reply::Json(200, reply);
    }
    frames(vec![
        json!({"choices": [{"index": 0, "delta": {"role": "assistant", "content": text}, "finish_reason": null}]}),
        json!({"choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]}),
        json!({"choices": [], "usage": usage()}),
    ])
}
