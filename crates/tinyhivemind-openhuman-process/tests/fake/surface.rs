//! A scripted fake of `openhuman-core run --headless-api`: the JSON-RPC,
//! `/health` and `/events` surface the runner uses, with the field names and
//! behaviour recorded from 0.64.15 in the harness spike.
//!
//! What it copies, because the runner must cope with it:
//!
//! - bad or unreadable TOML is not an error: the instance runs on defaults;
//! - MCP tools are listed only after a warm-up, by hashed names
//!   `mcp_<server>_<tool>_<hex>`;
//! - `channel_web_chat` rejects unknown params, answers with a `request_id`,
//!   and may publish events before that answer arrives;
//! - events go to `/events` subscribers of the turn's `client_id`, and an
//!   `approval_request` carries the approval's id as its `request_id`;
//! - a new message on a busy thread interrupts the turn on it;
//! - the custom agent ends after eight iterations as if it had finished;
//! - compaction is on unless the config turns it off.
//!
//! It is driven through files in a control directory shared with the test:
//! `steps/` (the scripted model, in name order), `calls.jsonl` (each model
//! call and its end), `served` (the tools the host's server serves),
//! `tools` (`extra` or `missing`), `misconfigured`, `version`, `warm`
//! (available-tools calls before MCP tools appear), and `methods.jsonl` and
//! `decisions.jsonl`, which it writes.
use super::http::{Reply, Request, event, serve};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tokio::net::TcpListener;
use tokio::sync::{Notify, mpsc, oneshot};

/// The version the fake reports unless told otherwise.
pub(crate) const VERSION: &str = "0.64.15";
/// The released binary's custom-agent iteration cap.
const CAP: u64 = 8;

/// What the fake is started with.
#[derive(Clone, Debug)]
pub(crate) struct Setup {
    /// The bearer every `/rpc` and `/events` request must carry.
    pub(crate) token: String,
    /// `OPENHUMAN_WORKSPACE`: holds `config.toml` and the transcripts.
    pub(crate) workspace: PathBuf,
    /// The control directory.
    pub(crate) control: PathBuf,
    /// The pid `/health` reports.
    pub(crate) pid: u32,
}

/// A running fake.
pub(crate) struct Fake {
    /// Where it listens, on 127.0.0.1.
    pub(crate) port: u16,
    task: tokio::task::JoinHandle<()>,
    state: Arc<State>,
}

impl Fake {
    /// Serve `setup` on a free port.
    pub(crate) async fn start(setup: Setup) -> std::io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        Ok(Self::on(listener, setup, port))
    }

    /// Serve `setup` on `listener`.
    pub(crate) fn on(listener: TcpListener, setup: Setup, port: u16) -> Self {
        let state = Arc::new(State::new(setup));
        let serving = state.clone();
        let task = tokio::spawn(serve(listener, move |request| {
            let state = serving.clone();
            async move { state.handle(&request) }
        }));
        Self { port, task, state }
    }
}

impl Drop for Fake {
    /// Like a process that is gone: nothing answers and every stream ends.
    fn drop(&mut self) {
        self.task.abort();
        lock(&self.state.subscribers).clear();
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A turn running on a thread.
struct InFlight {
    request_id: String,
    cancel: Arc<Notify>,
}

struct State {
    setup: Setup,
    config: Value,
    subscribers: Mutex<Vec<(String, mpsc::Sender<String>)>>,
    in_flight: Mutex<BTreeMap<String, InFlight>>,
    approvals: Mutex<BTreeMap<String, oneshot::Sender<String>>>,
    listed: AtomicUsize,
    next: AtomicU64,
}

impl State {
    fn new(setup: Setup) -> Self {
        let config = effective(&setup);
        Self {
            setup,
            config,
            subscribers: Mutex::default(),
            in_flight: Mutex::default(),
            approvals: Mutex::default(),
            listed: AtomicUsize::new(0),
            next: AtomicU64::new(0),
        }
    }

    fn control(&self, name: &str) -> PathBuf {
        self.setup.control.join(name)
    }

    fn read(&self, name: &str) -> Option<String> {
        fs::read_to_string(self.control(name)).ok()
    }

    fn append(&self, name: &str, entry: &Value) {
        let file = fs::File::options()
            .create(true)
            .append(true)
            .open(self.control(name));
        if let Ok(mut file) = file {
            let _ = writeln!(file, "{entry}");
        }
    }

    fn handle(self: Arc<Self>, request: &Request) -> Reply {
        if request.method == "GET" && request.path == "/health" {
            let health = json!({"pid": self.setup.pid, "healthy": true, "components": {}});
            return Reply::Json(200, health);
        }
        let bearer = format!("Bearer {}", self.setup.token);
        if request.header("authorization") != Some(bearer.as_str()) {
            return Reply::Json(401, json!({"ok": false, "error": "unauthorized"}));
        }
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/events") => self.subscribe(request),
            ("POST", "/rpc") => {
                let body = request.json();
                let id = body["id"].clone();
                let method = body["method"].as_str().unwrap_or_default().to_owned();
                self.append("methods.jsonl", &json!(method));
                let params = body["params"].clone();
                let answer = match self.clone().call(&method, &params) {
                    Ok(result) => json!({"jsonrpc": "2.0", "id": id, "result": result}),
                    Err(message) => json!({"jsonrpc": "2.0", "id": id,
                        "error": {"code": -32000, "message": message, "data": null}}),
                };
                Reply::Json(200, answer)
            }
            _ => Reply::Json(404, json!({"ok": false, "error": "not_found"})),
        }
    }

    fn subscribe(&self, request: &Request) -> Reply {
        let (sender, receiver) = mpsc::channel(1024);
        let client = request.param("client_id").unwrap_or_default();
        lock(&self.subscribers).push((client, sender));
        Reply::Stream(receiver)
    }

    /// Publish `name` to the subscribers of `client`.
    fn publish(&self, client: &str, thread: &str, request_id: &str, name: &str, fields: Value) {
        let mut data = json!({
            "event": name, "client_id": client, "thread_id": thread, "request_id": request_id,
        });
        if let (Some(data), Value::Object(fields)) = (data.as_object_mut(), fields) {
            data.extend(fields);
        }
        let frame = event(name, &data);
        let mut subscribers = lock(&self.subscribers);
        subscribers.retain(|(_, sender)| !sender.is_closed());
        for (_, sender) in subscribers.iter().filter(|(id, _)| id == client) {
            let _ = sender.try_send(frame.clone());
        }
    }

    fn call(self: Arc<Self>, method: &str, params: &Value) -> Result<Value, String> {
        let envelope = |result: Value| json!({"logs": ["fake"], "result": result});
        match method {
            "core.version" => {
                let version = self.read("version").unwrap_or_else(|| VERSION.into());
                Ok(json!({"version": version.trim()}))
            }
            "openhuman.config_get" => Ok(envelope(json!({
                "config": self.config,
                "workspace_dir": self.setup.workspace.join("workspace"),
            }))),
            "openhuman.agent_registry_available_tools" => Ok(json!({"tools": self.tools()})),
            "openhuman.threads_transcript_get" => {
                let thread = params["thread_id"].as_str().unwrap_or_default();
                let exists = self.transcript(thread).exists();
                Ok(json!({"data": {"threadId": thread, "hasTranscript": exists}}))
            }
            "openhuman.channel_web_chat" => self.chat(params).map(envelope),
            "openhuman.channel_web_cancel" => Ok(envelope(self.cancel(params))),
            "openhuman.approval_decide" => {
                let id = params["request_id"].as_str().unwrap_or_default();
                let decision = params["decision"].as_str().unwrap_or_default().to_owned();
                self.append("decisions.jsonl", &json!({"id": id, "decision": decision}));
                let pending = lock(&self.approvals).remove(id);
                let sender = pending.ok_or_else(|| format!("no pending approval {id}"))?;
                let _ = sender.send(decision);
                Ok(envelope(json!({"request_id": id})))
            }
            other => Err(format!("unknown method: {other}")),
        }
    }

    fn tools(&self) -> Vec<Value> {
        let builtin = ["shell", "mcp_call_tool", "juice_retrieve"];
        let mut tools: Vec<String> = builtin.map(String::from).to_vec();
        let warm: usize = self
            .read("warm")
            .and_then(|n| n.trim().parse().ok())
            .unwrap_or(2);
        if self.listed.fetch_add(1, Ordering::Relaxed) >= warm {
            let mut served: Vec<String> = self
                .read("served")
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or_default();
            if self.read("tools").as_deref() == Some("missing") {
                served.pop();
            }
            let servers = self.config["mcp_client"]["servers"].as_array().cloned();
            for server in servers.unwrap_or_default() {
                let name = server["name"].as_str().unwrap_or_default();
                for tool in &served {
                    tools.push(hashed(name, tool));
                }
            }
        }
        let tools = tools
            .into_iter()
            .map(|name| json!({"name": name, "description": ""}));
        tools.collect()
    }

    fn transcript(&self, thread: &str) -> PathBuf {
        let safe: String = thread
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        self.setup
            .workspace
            .join("workspace/session_raw")
            .join(format!("{safe}.jsonl"))
    }

    fn chat(self: Arc<Self>, params: &Value) -> Result<Value, String> {
        let known = [
            "client_id",
            "thread_id",
            "message",
            "model_override",
            "queue_mode",
        ];
        for key in params.as_object().into_iter().flatten().map(|(key, _)| key) {
            if !known.contains(&key.as_str()) {
                return Err(format!("unknown param '{key}' for channel.web_chat"));
            }
        }
        let agent = self.config["agent"]["chat_agent_id"]
            .as_str()
            .unwrap_or("orchestrator");
        let entries = self.config["agent_registry"]["entries"].as_array().cloned();
        if !entries
            .unwrap_or_default()
            .iter()
            .any(|entry| entry["id"] == agent)
        {
            return Err(format!(
                "chat_agent_id '{agent}' is not a runnable agent definition"
            ));
        }
        let text = |key: &str| {
            params[key]
                .as_str()
                .filter(|text| !text.is_empty())
                .map(str::to_owned)
        };
        let (Some(client), Some(thread), Some(message)) =
            (text("client_id"), text("thread_id"), text("message"))
        else {
            return Err("client_id, thread_id and message are required".into());
        };
        let number = self.next.fetch_add(1, Ordering::Relaxed);
        let request_id = format!("req-{number}");
        let cancel = Arc::new(Notify::new());
        let flight = InFlight {
            request_id: request_id.clone(),
            cancel: cancel.clone(),
        };
        if let Some(earlier) = lock(&self.in_flight).insert(thread.clone(), flight) {
            // A second message interrupts the first, as `queue_mode` does by default.
            earlier.cancel.notify_one();
        }
        // The first event is out before the answer is.
        let first = json!({"message": format!("Iteration 1/{CAP}"), "round": 1});
        self.publish(&client, &thread, &request_id, "iteration_start", first);
        let ack = json!({"accepted": true, "client_id": client, "thread_id": thread,
            "request_id": request_id});
        let ids = Ids {
            client,
            thread,
            request_id,
        };
        tokio::spawn(async move { self.run(ids, message, cancel).await });
        Ok(ack)
    }

    fn cancel(&self, params: &Value) -> Value {
        let thread = params["thread_id"].as_str().unwrap_or_default();
        let scoped = params["request_id"].as_str();
        let mut in_flight = lock(&self.in_flight);
        let target = in_flight
            .get(thread)
            .is_some_and(|flight| scoped.is_none_or(|id| id == flight.request_id));
        let cancelled = if target {
            in_flight.remove(thread)
        } else {
            None
        };
        let request_id = cancelled.as_ref().map(|flight| flight.request_id.clone());
        if let Some(flight) = cancelled {
            flight.cancel.notify_one();
        }
        json!({"cancelled": request_id.is_some(), "client_id": params["client_id"],
            "thread_id": thread, "request_id": request_id, "subagents_cancelled": 0})
    }

    /// The next scripted step; null hangs.
    fn step(&self) -> Value {
        let mut steps: Vec<PathBuf> = fs::read_dir(self.control("steps"))
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .collect();
        steps.sort();
        for next in steps {
            if let Ok(text) = fs::read_to_string(&next)
                && fs::remove_file(&next).is_ok()
            {
                return serde_json::from_str(&text).unwrap_or(Value::Null);
            }
        }
        Value::Null
    }

    async fn run(self: Arc<Self>, ids: Ids, message: String, cancel: Arc<Notify>) {
        let step = self.step();
        let say = |name: &str, fields: Value| {
            self.publish(&ids.client, &ids.thread, &ids.request_id, name, fields);
        };
        if step["stray"].as_bool() == Some(true) {
            let stray = json!({"full_response": "not yours", "request_id": "req-stray"});
            self.publish(&ids.client, &ids.thread, "req-stray", "chat_done", stray);
        }
        if let Some(tool) = step["approval"].as_str() {
            let id = format!("approval-{}", self.next.fetch_add(1, Ordering::Relaxed));
            let (sender, decided) = oneshot::channel();
            lock(&self.approvals).insert(id.clone(), sender);
            // Its `request_id` is the approval's, not the turn's.
            let request = json!({"tool_name": tool, "args": {"command": "<redacted>"}});
            self.publish(&ids.client, &ids.thread, &id, "approval_request", request);
            let _ = decided.await;
        }
        let iterations = step["iterations"].as_u64().unwrap_or(1).min(CAP);
        for round in 2..=iterations {
            let text = format!("Iteration {round}/{CAP}");
            say("iteration_start", json!({"message": text, "round": round}));
        }
        if let Some(tool) = step["tool"].as_str() {
            let call = json!({"tool_name": tool, "tool_call_id": "call_1", "args": {}});
            say("tool_call", call);
            let result = json!({"tool_name": tool, "tool_call_id": "call_1", "success": false});
            say("tool_result", result);
        }
        // Only a turn that ends is kept in the thread, as 0.64.15 does.
        let seen = self.users(&ids.thread) + 1;
        let call = self.open(seen);
        if step.is_null() {
            cancel.notified().await;
            self.append("calls.jsonl", &json!({"call": call, "state": "aborted"}));
            say(
                "chat_error",
                json!({"message": "Cancelled", "error_type": "cancelled"}),
            );
            return;
        }
        self.append("calls.jsonl", &json!({"call": call, "state": "answered"}));
        self.finish(&ids.thread, &ids.request_id);
        if let Some(error) = step["error"].as_str() {
            say(
                "chat_error",
                json!({"message": error, "error_type": "inference"}),
            );
            return;
        }
        let reply = step["reply"].as_str().unwrap_or_default();
        self.remember(&ids.thread, "user", &message);
        self.remember(&ids.thread, "assistant", reply);
        let tokens = step["tokens"].as_u64().unwrap_or(0);
        let usage = json!({"input_tokens": tokens, "output_tokens": 2,
            "cached_input_tokens": tokens / 10, "cost_usd": 0.0, "context_window": 0});
        say(
            "text_delta",
            json!({"delta": reply, "delta_kind": "text", "round": 1}),
        );
        say("turn_cost", json!({"usage": usage, "round": 1}));
        say("chat_done", json!({"full_response": reply, "usage": usage}));
    }

    /// The turn on `thread` ended, unless a newer one replaced it.
    fn finish(&self, thread: &str, request_id: &str) {
        let mut in_flight = lock(&self.in_flight);
        if in_flight
            .get(thread)
            .is_some_and(|flight| flight.request_id == request_id)
        {
            in_flight.remove(thread);
        }
    }

    /// The user messages in the thread's transcript.
    fn users(&self, thread: &str) -> usize {
        let text = fs::read_to_string(self.transcript(thread)).unwrap_or_default();
        text.lines()
            .filter(|line| line.contains("\"user\""))
            .count()
    }

    /// Append to the thread's transcript.
    fn remember(&self, thread: &str, role: &str, text: &str) {
        let path = self.transcript(thread);
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let file = fs::File::options().create(true).append(true).open(&path);
        if let Ok(mut file) = file {
            let _ = writeln!(file, "{}", json!({"role": role, "text": text}));
        }
    }

    fn open(&self, seen: usize) -> u64 {
        let log = self.read("calls.jsonl").unwrap_or_default();
        let count = log.lines().filter(|line| line.contains("\"open\"")).count();
        let call = u64::try_from(count).unwrap_or(u64::MAX);
        let entry = json!({"call": call, "state": "open", "seen": seen,
            "pid": std::process::id()});
        self.append("calls.jsonl", &entry);
        call
    }
}

struct Ids {
    client: String,
    thread: String,
    request_id: String,
}

/// The model-visible name 0.64.15 gives a direct MCP tool.
pub(crate) fn hashed(server: &str, tool: &str) -> String {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in format!("{server}/{tool}").bytes() {
        hash = (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193);
    }
    format!("mcp_{server}_{tool}_{:06x}", hash & 0x00ff_ffff)
}

/// The config the instance runs with: the file, unless it cannot be read,
/// with some of the defaults 0.64.15 adds.
fn effective(setup: &Setup) -> Value {
    let control = &setup.control;
    let written = fs::read_to_string(setup.workspace.join("config.toml")).ok();
    let parsed = written
        .filter(|_| !control.join("misconfigured").exists())
        .and_then(|text| text.parse::<toml::Table>().ok())
        .and_then(|table| serde_json::to_value(table).ok());
    let mut config = parsed.unwrap_or_else(|| json!({"schema_version": 13}));
    config["embeddings_provider"] = json!("openhuman");
    if config["context"]["compaction_enabled"].is_null() {
        config["context"]["compaction_enabled"] = json!(true);
    }
    if let Some(servers) = config["mcp_client"]["servers"].as_array_mut() {
        for server in servers {
            server["timeout_secs"] = json!(30);
        }
    }
    if fs::read_to_string(control.join("tools")).is_ok_and(|text| text == "extra") {
        let entries = config["agent_registry"]["entries"].as_array_mut();
        for entry in entries.into_iter().flatten() {
            if let Some(tools) = entry["tool_allowlist"].as_array_mut() {
                tools.push(json!("shell"));
            }
        }
    }
    config
}

/// A control directory for one fake, with its `steps/`.
pub(crate) fn control(root: &Path) -> std::io::Result<PathBuf> {
    let control = root.join("control");
    fs::create_dir_all(control.join("steps"))?;
    Ok(control)
}
