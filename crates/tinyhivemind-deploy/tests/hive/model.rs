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
//!
//! A model started with a [`Delegation`] scripts a lead-only episode
//! instead: the lead asks the seats named with `hivemind_ask` and ends its
//! turn, each seat asked completes in the conversation the ask opened, and
//! the lead completes once a turn shows it every answer. Any other seat
//! only answers in plain text.
#[expect(
    dead_code,
    reason = "shared with the OpenHuman runner's fakes; this model reads bodies only"
)]
#[path = "../../../tinyhivemind-openhuman-process/tests/support/http.rs"]
mod http;

use http::{Reply, Request, serve};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
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

/// What seat `agent` answers when it is asked.
pub(crate) fn answered(agent: &str) -> String {
    format!("{agent} answered")
}

/// A lead-only episode: `lead` asks `asked` in one conversation.
#[derive(Clone, Debug)]
pub(crate) struct Delegation {
    pub(crate) lead: String,
    pub(crate) asked: Vec<String>,
}

/// An id never used before, for a response or a tool call. Claude Code
/// joins the stored parts of one response by its id, so two responses
/// sharing one would be joined into a single message, out of order, on the
/// seat's next turn; and it interrupts a tool call whose id its session has
/// already seen.
fn fresh(prefix: &str) -> String {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    format!("{prefix}_{}", NEXT.fetch_add(1, Ordering::Relaxed))
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
    /// The conversation the turn ran in, absent on the open hive.
    pub(crate) thread: Option<u64>,
    /// The rendered turn, as the seat was shown it.
    pub(crate) context: String,
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
        Self::serving(Script::default()).await
    }

    /// The model, whose editing seats aim their absolute write at `outside`.
    pub(crate) async fn start_with(outside: &str) -> std::io::Result<Self> {
        Self::serving(Script {
            outside: outside.to_owned(),
            delegation: None,
        })
        .await
    }

    /// The model, scripting the lead-only episode `delegation`.
    pub(crate) async fn start_delegating(delegation: Delegation) -> std::io::Result<Self> {
        Self::serving(Script {
            delegation: Some(delegation),
            ..Script::default()
        })
        .await
    }

    async fn serving(script: Script) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let state = State {
            seen: seen.clone(),
            script: Arc::new(script),
            asked_in: Arc::new(Mutex::new(None)),
        };
        let task = tokio::spawn(serve(listener, move |request| {
            let state = state.clone();
            async move { answer(&state, &request) }
        }));
        Ok(Self { port, seen, task })
    }

    /// Every request that carried a turn.
    pub(crate) fn seen(&self) -> Vec<Seen> {
        lock(&self.seen).clone()
    }
}

/// What the model plays: an editing seat's `outside` aim, or a delegation.
struct Script {
    outside: String,
    delegation: Option<Delegation>,
}

impl Default for Script {
    fn default() -> Self {
        Self {
            outside: "/nonexistent".to_owned(),
            delegation: None,
        }
    }
}

/// The model's shared state, one clone per request.
#[derive(Clone)]
struct State {
    seen: Arc<Mutex<Vec<Seen>>>,
    script: Arc<Script>,
    /// The turn a delegation's lead asked in.
    asked_in: Arc<Mutex<Option<String>>>,
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

/// A tool the request offers, with its namespace, by the end of its name.
type Offered = (Option<String>, String);

fn answer(state: &State, request: &Request) -> Reply {
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
    let Some(turn) = read.turn.clone().filter(|_| !read.offered.is_empty()) else {
        return say(api, "ok", stream);
    };
    let agent = turn["agent_id"].as_str().unwrap_or_default().to_owned();
    lock(&state.seen).push(Seen {
        api,
        agent: agent.clone(),
        turn: turn["turn_id"].as_str().unwrap_or_default().to_owned(),
        thread: turn["episode"]["thread"].as_u64(),
        context: turn.to_string(),
        offered: read.offered.iter().map(|(_, name)| name.clone()).collect(),
        results: read.results.clone(),
    });
    let step = match &state.script.delegation {
        Some(delegation) => delegating(delegation, &state.asked_in, &agent, &turn, &read),
        None => reporting(&state.script.outside, &agent, &turn, &read),
    };
    match step {
        Some(((namespace, name), arguments)) => {
            // A fresh id: Claude Code takes a call whose id its session has
            // seen before as already done and interrupts it.
            let id = fresh(&format!("call_{agent}"));
            call(api, &id, namespace.as_deref(), &name, &arguments, stream)
        }
        None => say(api, &format!("{agent} finished"), stream),
    }
}

/// The tool `read` offers whose name contains `name`.
fn find(read: &Read, name: &str) -> Option<Offered> {
    read.offered
        .iter()
        .find(|(_, tool)| tool.contains(name))
        .cloned()
}

/// A reporting seat's next call: post (twice, or once and then the edits
/// `EDITS` lists for a seat offered the edit tool), then complete.
fn reporting(outside: &str, agent: &str, turn: &Value, read: &Read) -> Option<(Offered, Value)> {
    let turn_id = turn["turn_id"].as_str().unwrap_or_default();
    let episode = turn["episode"]["episode_id"].as_str()?;
    let edit = find(read, EDIT_TOOL);
    let posts = if edit.is_some() { 1 } else { 2 };
    let complete_at = posts + edit.as_ref().map_or(0, |_| EDITS.len());
    match read.results.len() {
        at if at < posts => find(read, "hivemind_post").map(|tool| {
            let body = format!("{agent} reporting");
            (
                tool,
                json!({"episode_id": episode, "body": body, "turn": turn_id, "call": "post"}),
            )
        }),
        at if at < complete_at => edit.map(|tool| {
            let (path, call) = match EDITS[at - posts] {
                "escape" => (format!("../{agent}-escape.md"), "escape"),
                "absolute" => (format!("{outside}/{agent}.md"), "absolute"),
                // The retry repeats the edit's call exactly.
                _ => (format!("{agent}.md"), "edit"),
            };
            let content = edited(agent);
            (
                tool,
                json!({"path": path, "content": content, "turn": turn_id, "call": call}),
            )
        }),
        at if at == complete_at => find(read, "hivemind_complete").map(|tool| {
            let body = format!("{agent} done");
            (
                tool,
                json!({"episode_id": episode, "body": body, "turn": turn_id, "call": "done"}),
            )
        }),
        _ => None,
    }
}

/// A delegation's next call, one per turn and then a plain answer. The
/// lead completes once its turn shows every answer, and otherwise asks in
/// its first turn only (`asked_in` keeps which; a harness that resends that
/// turn's request asks again, and the host answers from its record). A seat
/// asked completes in the conversation. Any other seat says nothing.
fn delegating(
    delegation: &Delegation,
    asked_in: &Mutex<Option<String>>,
    agent: &str,
    turn: &Value,
    read: &Read,
) -> Option<(Offered, Value)> {
    if !read.results.is_empty() {
        return None;
    }
    let turn_id = turn["turn_id"].as_str().unwrap_or_default();
    let episode = turn["episode"]["episode_id"].as_str()?;
    let context = turn.to_string();
    if agent == delegation.lead {
        let every_answer = delegation
            .asked
            .iter()
            .all(|asked| context.contains(&answered(asked)));
        if every_answer {
            return find(read, "hivemind_complete").map(|tool| {
                let body = format!("{agent} done");
                (
                    tool,
                    json!({"episode_id": episode, "body": body, "turn": turn_id, "call": "done"}),
                )
            });
        }
        let mut asked_in = lock(asked_in);
        if asked_in.get_or_insert_with(|| turn_id.to_owned()) != turn_id {
            return None;
        }
        return find(read, "hivemind_ask").map(|tool| {
            (
                tool,
                json!({"episode_id": episode, "agents": delegation.asked, "body": "question",
                    "turn": turn_id, "call": "ask"}),
            )
        });
    }
    if delegation.asked.iter().any(|asked| asked == agent) && turn["episode"]["thread"].is_u64() {
        return find(read, "hivemind_complete").map(|tool| {
            let body = answered(agent);
            (
                tool,
                json!({"episode_id": episode, "body": body, "turn": turn_id, "call": "answer"}),
            )
        });
    }
    None
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
                    json!({"id": fresh("chatcmpl"), "object": "chat.completion", "created": 1, "model": "mock",
                    "choices": [{"index": 0, "finish_reason": "tool_calls", "message": {"role": "assistant", "content": null, "tool_calls": calls}}], "usage": usage}),
                );
            }
            let id = fresh("chatcmpl");
            let chunk = |choices: Value, usage: Option<Value>| {
                let mut chunk = json!({"id": id, "object": "chat.completion.chunk", "created": 1, "model": "mock", "choices": choices});
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
                    json!({"id": fresh("chatcmpl"), "object": "chat.completion", "created": 1, "model": "mock",
                    "choices": [{"index": 0, "finish_reason": "stop", "message": {"role": "assistant", "content": text}}], "usage": usage}),
                );
            }
            let base = json!({"id": fresh("chatcmpl"), "object": "chat.completion.chunk", "created": 1, "model": "mock"});
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
            let item = json!({"type": "message", "id": fresh("msg"), "role": "assistant", "status": "completed",
                "content": [{"type": "output_text", "text": text, "annotations": []}]});
            responses(&item, stream)
        }
        Api::Messages => messages(&json!({"type": "text", "text": text}), "end_turn", stream),
    }
}

fn responses(item: &Value, stream: bool) -> Reply {
    let usage = json!({"input_tokens": 100, "input_tokens_details": {"cached_tokens": 10}, "output_tokens": 7,
        "output_tokens_details": {"reasoning_tokens": 0}, "total_tokens": 107});
    let response = json!({"id": fresh("resp"), "object": "response", "status": "completed", "model": "mock", "output": [item], "usage": usage});
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
    let message = json!({"id": fresh("msg"), "type": "message", "role": "assistant", "model": "mock", "content": [block],
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
