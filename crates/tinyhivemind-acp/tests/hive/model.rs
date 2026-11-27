//! A model on loopback that acts as a hive seat would, in the three APIs the
//! harnesses speak: Chat Completions (`OpenHuman`), Responses (Codex) and
//! Anthropic Messages (Claude Code).
//!
//! A request carries a seat's turn when one of its user texts is the
//! rendered `TurnRequest`. The model reads the turn and episode from it and,
//! counting the tool results after it, calls `hivemind_post` with call
//! `post` and the same call again, then `hivemind_complete` with call
//! `done`, then answers. A seat offered the host's `write_file` posts once
//! and makes the edits [`EDITS`] lists before it completes, so that its turn
//! stays within `OpenHuman`'s cap of eight model calls. Any other request, or
//! one that offers no tools (a harness's own side call), gets a plain answer
//! and changes nothing.
#[expect(
    dead_code,
    reason = "shared with the OpenHuman runner's fakes; this model reads bodies only"
)]
#[path = "../../../tinyhivemind-openhuman-process/tests/support/http.rs"]
mod http;

use http::{Reply, Request, serve};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tokio::net::TcpListener;
use tokio::sync::mpsc;

/// The phrase that opens a rendered turn.
const TURN: &str = "Incoming attributed Hivemind context";

/// The host's edit tool, as the hive test declares it.
pub(crate) const EDIT_TOOL: &str = "write_file";

/// What an editing seat writes, in order, after its one post: its own file
/// with call `edit`, the same call again, a path climbing out of its
/// workspace, and an absolute path into the model's `outside` directory.
/// Their results are a turn's results from index 1.
pub(crate) const EDITS: [&str; 4] = ["edit", "retry", "escape", "absolute"];

/// What seat `agent` writes to its file.
pub(crate) fn edited(agent: &str) -> String {
    format!("{agent} was here\n")
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The API a request spoke.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Api {
    Chat,
    Responses,
    Messages,
}

/// What one request of a turn showed.
#[derive(Clone, Debug)]
pub(crate) struct Seen {
    pub(crate) api: Api,
    pub(crate) agent: String,
    pub(crate) turn: String,
    pub(crate) offered: Vec<String>,
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
        Self::start_with("/nonexistent").await
    }

    /// The model, whose editing seats aim their absolute write at `outside`.
    pub(crate) async fn start_with(outside: &str) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let serving = seen.clone();
        let aim = outside.to_owned();
        let task = tokio::spawn(serve(listener, move |request| {
            let seen = serving.clone();
            let outside = aim.clone();
            async move { answer(&seen, &outside, &request) }
        }));
        Ok(Self { port, seen, task })
    }

    /// Every request that carried a turn.
    pub(crate) fn seen(&self) -> Vec<Seen> {
        lock(&self.seen).clone()
    }
}

impl Drop for HiveModel {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// The text of a `content`: a string, or the text of its parts.
fn text(content: &Value) -> String {
    match content {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts
            .iter()
            .map(|part| text(&part["text"]) + &text(&part["content"]))
            .collect(),
        _ => String::new(),
    }
}

/// A tool result without the timing a harness adds, which differs between
/// two runs of the same call: Codex's before it (`Wall time: … seconds\n
/// Output:…`) and `OpenHuman`'s after it (`…\n\n[took 0.1s]`).
fn tool_output(text: &str) -> String {
    let text = match (text.starts_with("Wall time:"), text.split_once("Output:")) {
        (true, Some((_, output))) => output,
        _ => text,
    };
    match text.rsplit_once("\n\n[took ") {
        Some((output, timing)) if timing.ends_with("s]") => output.to_owned(),
        _ => text.to_owned(),
    }
}

/// The rendered turn in `text`: the JSON after the phrase.
fn turn_of(text: &str) -> Option<Value> {
    let at = text.find(TURN)?;
    let start = at + text[at..].find('{')?;
    let mut stream = serde_json::Deserializer::from_str(&text[start..]).into_iter::<Value>();
    stream.next()?.ok()
}

/// One request, read alike in each API: the turn, the tools offered (with a
/// namespace, for Responses) and the tool results after the turn.
struct Read {
    turn: Option<Value>,
    offered: Vec<(Option<String>, String)>,
    results: Vec<String>,
}

fn read(api: Api, body: &Value) -> Read {
    let items: Vec<Value> = match api {
        Api::Responses => match &body["input"] {
            Value::Array(items) => items.clone(),
            other => vec![json!({"role": "user", "content": other})],
        },
        Api::Chat | Api::Messages => body["messages"].as_array().cloned().unwrap_or_default(),
    };
    let mut turn = None;
    let mut at = 0;
    for (index, item) in items.iter().enumerate() {
        if item["role"] == "user"
            && let Some(found) = turn_of(&text(&item["content"]))
        {
            turn = Some(found);
            at = index;
        }
    }
    let after = items.get(at + 1..).unwrap_or_default();
    let results = match api {
        Api::Chat => after
            .iter()
            .filter(|item| item["role"] == "tool")
            .map(|item| tool_output(&text(&item["content"])))
            .collect(),
        Api::Responses => after
            .iter()
            .filter(|item| item["type"] == "function_call_output")
            .map(|item| tool_output(&text(&item["output"])))
            .collect(),
        Api::Messages => after
            .iter()
            .flat_map(|item| item["content"].as_array().cloned().unwrap_or_default())
            .filter(|block| block["type"] == "tool_result")
            .map(|block| tool_output(&text(&block["content"])))
            .collect(),
    };
    let offered = body["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|tool| match (tool["type"].as_str(), api) {
            (Some("namespace"), _) => tool["tools"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|inner| inner["name"].as_str())
                .map(|name| (tool["name"].as_str().map(str::to_owned), name.to_owned()))
                .collect::<Vec<_>>(),
            (_, Api::Chat) => tool["function"]["name"]
                .as_str()
                .map(|name| (None, name.to_owned()))
                .into_iter()
                .collect(),
            _ => tool["name"]
                .as_str()
                .map(|name| (None, name.to_owned()))
                .into_iter()
                .collect(),
        })
        .collect();
    Read {
        turn: turn.filter(|turn| turn["turn_id"].is_string()),
        offered,
        results,
    }
}

fn answer(seen: &Arc<Mutex<Vec<Seen>>>, outside: &str, request: &Request) -> Reply {
    let path = request.path.as_str();
    if request.method == "GET" && path.ends_with("/models") {
        return Reply::Json(
            200,
            json!({"object": "list", "data": [{"id": "mock", "object": "model"}]}),
        );
    }
    if path.contains("/count_tokens") {
        return Reply::Json(200, json!({"input_tokens": 100}));
    }
    let api = if path.ends_with("/chat/completions") {
        Api::Chat
    } else if path.ends_with("/responses") {
        Api::Responses
    } else if path.contains("/v1/messages") {
        Api::Messages
    } else {
        return Reply::Json(404, json!({}));
    };
    let body = request.json();
    let stream = body["stream"] == true;
    let read = read(api, &body);
    // A harness's side call (a session title, say) offers no tools: it gets a
    // plain answer and is not part of the turn.
    let Some(turn) = read.turn.filter(|_| !read.offered.is_empty()) else {
        return say(api, "ok", stream);
    };
    let agent = turn["agent_id"].as_str().unwrap_or_default().to_owned();
    let turn_id = turn["turn_id"].as_str().unwrap_or_default().to_owned();
    let episode = turn["episode"]["episode_id"].as_str().map(str::to_owned);
    lock(seen).push(Seen {
        api,
        agent: agent.clone(),
        turn: turn_id.clone(),
        offered: read.offered.iter().map(|(_, name)| name.clone()).collect(),
        results: read.results.clone(),
    });
    let find = |name: &str| {
        read.offered
            .iter()
            .find(|(_, tool)| tool.contains(name))
            .cloned()
    };
    // A seat offered the edit tool posts once, then makes the edits `EDITS`
    // lists before it completes.
    let edit = find(EDIT_TOOL);
    let posts = if edit.is_some() { 1 } else { 2 };
    let complete_at = posts + edit.as_ref().map_or(0, |_| EDITS.len());
    let step = match (episode, read.results.len()) {
        (Some(episode), at) if at < posts => find("hivemind_post").map(|tool| {
            let body = format!("{agent} reporting");
            (
                tool,
                json!({"episode_id": episode, "body": body, "turn": turn_id, "call": "post"}),
            )
        }),
        (Some(_), at) if at >= posts && at < complete_at => edit.map(|tool| {
            let (path, call) = match EDITS[at - posts] {
                "escape" => (format!("../{agent}-escape.md"), "escape"),
                "absolute" => (format!("{outside}/{agent}.md"), "absolute"),
                // The retry repeats the edit's call exactly.
                _ => (format!("{agent}.md"), "edit"),
            };
            let content = edited(&agent);
            (
                tool,
                json!({"path": path, "content": content, "turn": turn_id, "call": call}),
            )
        }),
        (Some(episode), at) if at == complete_at => find("hivemind_complete").map(|tool| {
            let body = format!("{agent} done");
            (
                tool,
                json!({"episode_id": episode, "body": body, "turn": turn_id, "call": "done"}),
            )
        }),
        _ => None,
    };
    match step {
        Some(((namespace, name), arguments)) => {
            let id = format!("call_{}_{}", agent, read.results.len());
            call(api, &id, namespace.as_deref(), &name, &arguments, stream)
        }
        None => say(api, &format!("{agent} finished"), stream),
    }
}

fn sse(events: Vec<(Option<&str>, Value)>, done: bool) -> Reply {
    let (sender, receiver) = mpsc::channel(32);
    let mut frames: Vec<String> = events
        .into_iter()
        .map(|(event, data)| match event {
            Some(event) => format!("event: {event}\ndata: {data}\n\n"),
            None => format!("data: {data}\n\n"),
        })
        .collect();
    if done {
        frames.push("data: [DONE]\n\n".to_owned());
    }
    tokio::spawn(async move {
        for frame in frames {
            let _ = sender.send(frame).await;
        }
    });
    Reply::Stream(receiver)
}

/// A tool call, in `api`.
fn call(
    api: Api,
    id: &str,
    namespace: Option<&str>,
    name: &str,
    arguments: &Value,
    stream: bool,
) -> Reply {
    let arguments = arguments.to_string();
    match api {
        Api::Chat => {
            let calls = json!([{"index": 0, "id": id, "type": "function", "function": {"name": name, "arguments": arguments}}]);
            let usage = json!({"prompt_tokens": 100, "completion_tokens": 7, "total_tokens": 107});
            if !stream {
                return Reply::Json(
                    200,
                    json!({"id": "c", "object": "chat.completion", "created": 1, "model": "mock",
                    "choices": [{"index": 0, "finish_reason": "tool_calls", "message": {"role": "assistant", "content": null, "tool_calls": calls}}], "usage": usage}),
                );
            }
            let chunk = |choices: Value, usage: Option<Value>| {
                let mut chunk = json!({"id": "c", "object": "chat.completion.chunk", "created": 1, "model": "mock", "choices": choices});
                if let Some(usage) = usage {
                    chunk["usage"] = usage;
                }
                (None, chunk)
            };
            sse(
                vec![
                    chunk(
                        json!([{"index": 0, "delta": {"role": "assistant", "tool_calls": calls}, "finish_reason": null}]),
                        None,
                    ),
                    chunk(
                        json!([{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]),
                        None,
                    ),
                    chunk(json!([]), Some(usage)),
                ],
                true,
            )
        }
        Api::Responses => {
            let mut item = json!({"type": "function_call", "id": format!("fc_{id}"), "call_id": id, "name": name, "arguments": arguments, "status": "completed"});
            if let Some(namespace) = namespace {
                item["namespace"] = namespace.into();
            }
            responses(&item, stream)
        }
        Api::Messages => {
            let input: Value = serde_json::from_str(&arguments).unwrap_or(Value::Null);
            let block = json!({"type": "tool_use", "id": id, "name": name, "input": input});
            messages(&block, "tool_use", stream)
        }
    }
}

/// A plain answer, in `api`.
fn say(api: Api, text: &str, stream: bool) -> Reply {
    match api {
        Api::Chat => {
            let usage = json!({"prompt_tokens": 100, "completion_tokens": 7, "total_tokens": 107});
            if !stream {
                return Reply::Json(
                    200,
                    json!({"id": "c", "object": "chat.completion", "created": 1, "model": "mock",
                    "choices": [{"index": 0, "finish_reason": "stop", "message": {"role": "assistant", "content": text}}], "usage": usage}),
                );
            }
            let base = json!({"id": "c", "object": "chat.completion.chunk", "created": 1, "model": "mock"});
            let with = |extra: Value| {
                let mut chunk = base.clone();
                if let (Some(chunk), Some(extra)) = (chunk.as_object_mut(), extra.as_object()) {
                    chunk.extend(extra.clone());
                }
                (None, chunk)
            };
            sse(
                vec![
                    with(
                        json!({"choices": [{"index": 0, "delta": {"role": "assistant", "content": text}, "finish_reason": null}]}),
                    ),
                    with(json!({"choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]})),
                    with(json!({"choices": [], "usage": usage})),
                ],
                true,
            )
        }
        Api::Responses => {
            let item = json!({"type": "message", "id": "msg_1", "role": "assistant", "status": "completed",
                "content": [{"type": "output_text", "text": text, "annotations": []}]});
            responses(&item, stream)
        }
        Api::Messages => messages(&json!({"type": "text", "text": text}), "end_turn", stream),
    }
}

fn responses(item: &Value, stream: bool) -> Reply {
    let usage = json!({"input_tokens": 100, "input_tokens_details": {"cached_tokens": 10}, "output_tokens": 7,
        "output_tokens_details": {"reasoning_tokens": 0}, "total_tokens": 107});
    let response = json!({"id": "resp_1", "object": "response", "status": "completed", "model": "mock", "output": [item], "usage": usage});
    if !stream {
        return Reply::Json(200, response);
    }
    let mut created = response.clone();
    created["status"] = "in_progress".into();
    created["output"] = json!([]);
    let mut added = item.clone();
    added["status"] = "in_progress".into();
    let mut events = vec![
        (
            Some("response.created"),
            json!({"type": "response.created", "response": created}),
        ),
        (
            Some("response.output_item.added"),
            json!({"type": "response.output_item.added", "output_index": 0, "item": added}),
        ),
    ];
    if item["type"] == "message" {
        let delta = item["content"][0]["text"].clone();
        events.push((Some("response.output_text.delta"), json!({"type": "response.output_text.delta", "item_id": item["id"], "output_index": 0, "content_index": 0, "delta": delta})));
    }
    events.push((
        Some("response.output_item.done"),
        json!({"type": "response.output_item.done", "output_index": 0, "item": item}),
    ));
    events.push((
        Some("response.completed"),
        json!({"type": "response.completed", "response": response}),
    ));
    sse(events, false)
}

fn messages(block: &Value, stop: &str, stream: bool) -> Reply {
    let usage = json!({"input_tokens": 100, "output_tokens": 7, "cache_read_input_tokens": 10, "cache_creation_input_tokens": 0});
    let message = json!({"id": "msg_1", "type": "message", "role": "assistant", "model": "mock", "content": [block],
        "stop_reason": stop, "stop_sequence": null, "usage": usage});
    if !stream {
        return Reply::Json(200, message);
    }
    let mut start = message.clone();
    start["content"] = json!([]);
    start["stop_reason"] = Value::Null;
    let (opened, delta) = if block["type"] == "tool_use" {
        let mut opened = block.clone();
        opened["input"] = json!({});
        (
            opened,
            json!({"type": "input_json_delta", "partial_json": block["input"].to_string()}),
        )
    } else {
        (
            json!({"type": "text", "text": ""}),
            json!({"type": "text_delta", "text": block["text"]}),
        )
    };
    let event = |kind: &'static str, mut data: Value| {
        data["type"] = kind.into();
        (Some(kind), data)
    };
    sse(
        vec![
            event("message_start", json!({"message": start})),
            event(
                "content_block_start",
                json!({"index": 0, "content_block": opened}),
            ),
            event("content_block_delta", json!({"index": 0, "delta": delta})),
            event("content_block_stop", json!({"index": 0})),
            event(
                "message_delta",
                json!({"delta": {"stop_reason": stop, "stop_sequence": null}, "usage": {"output_tokens": 7}}),
            ),
            event("message_stop", json!({})),
        ],
        false,
    )
}
