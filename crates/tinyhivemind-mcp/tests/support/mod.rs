//! A loopback HTTP client, and a hive whose seat `a` has a turn held open.
#![allow(dead_code, unreachable_pub, clippy::expect_used, clippy::unwrap_used)]
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{HeaderMap, Request, StatusCode};
use hyper_util::rt::TokioIo;
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tinyhivemind_exec::ExecContext;
use tinyhivemind_hives::{
    AgentRegistration, AgentRunner, Coordinator, CoordinatorOptions, Destination, HiveInfo,
    MemoryStorage, SendMessage, TurnDisposition, TurnFuture, TurnOutcome, TurnRequest,
};
use tinyhivemind_host::harness::ToolEndpoint;
use tinyhivemind_mcp::{HiveToolServer, SeatTools, ServerOptions, Services};
use tokio::sync::{Notify, mpsc};

/// What the server answered.
#[derive(Debug)]
pub struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Option<Value>,
}

/// Send `request` to `addr` on a fresh connection.
pub async fn send(addr: SocketAddr, request: Request<Full<Bytes>>) -> Reply {
    let stream = tokio::net::TcpStream::connect(addr).await.expect("connect");
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .expect("handshake");
    tokio::spawn(connection);
    let response = sender.send_request(request).await.expect("response");
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes();
    let body = (!bytes.is_empty()).then(|| serde_json::from_slice(&bytes).expect("json body"));
    Reply {
        status,
        headers,
        body,
    }
}

/// A well-formed MCP POST of `body` to `addr`, with `bearer` if given.
pub fn post(addr: SocketAddr, bearer: Option<&str>, body: &[u8]) -> hyper::http::request::Builder {
    let mut builder = Request::post("/mcp")
        .header("host", format!("127.0.0.1:{}", addr.port()))
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream");
    if let Some(bearer) = bearer {
        builder = builder.header("authorization", format!("Bearer {bearer}"));
    }
    let _ = body;
    builder
}

/// One seat's view of the server.
#[derive(Clone, Debug)]
pub struct Client {
    pub addr: SocketAddr,
    pub bearer: String,
}
impl Client {
    pub fn of(server: &HiveToolServer, endpoint: &ToolEndpoint) -> Self {
        Self {
            addr: server.local_addr(),
            bearer: endpoint.bearer().to_owned(),
        }
    }
    /// POST one JSON-RPC message and return what came back.
    pub async fn message(&self, message: &Value) -> Reply {
        let body = serde_json::to_vec(message).unwrap();
        let request = post(self.addr, Some(&self.bearer), &body)
            .body(Full::new(Bytes::from(body)))
            .unwrap();
        send(self.addr, request).await
    }
    /// Call `method` and return the whole JSON-RPC response.
    pub async fn rpc(&self, method: &str, params: Value) -> Value {
        let reply = self
            .message(&json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}))
            .await;
        assert_eq!(reply.status, StatusCode::OK, "{reply:?}");
        reply.body.expect("a response")
    }
    /// Call tool `name` with `arguments`: its text, and whether it is an error.
    pub async fn call(&self, name: &str, arguments: Value) -> (String, bool) {
        let response = self
            .rpc("tools/call", json!({"name": name, "arguments": arguments}))
            .await;
        let result = &response["result"];
        assert!(result.is_object(), "{response}");
        (
            result["content"][0]["text"].as_str().unwrap().to_owned(),
            result["isError"].as_bool().unwrap(),
        )
    }
    /// The served tools, by name, with their input schemas.
    pub async fn tools(&self) -> Vec<(String, Value)> {
        let response = self.rpc("tools/list", json!({})).await;
        response["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| {
                (
                    tool["name"].as_str().unwrap().to_owned(),
                    tool["inputSchema"].clone(),
                )
            })
            .collect()
    }
}

/// Hands its first turn to the test and holds it until told to finish.
struct Holds {
    started: mpsc::UnboundedSender<TurnRequest>,
    finish: Arc<Notify>,
}
impl AgentRunner for Holds {
    fn run(&self, request: TurnRequest) -> TurnFuture {
        let (started, finish) = (self.started.clone(), self.finish.clone());
        Box::pin(async move {
            if started.send(request.clone()).is_ok() {
                finish.notified().await;
            }
            Ok(TurnOutcome {
                session_id: format!("session:{}", request.agent_id),
                reply: None,
                disposition: TurnDisposition::Completed,
            })
        })
    }
}

/// A coordinator with seats `a` and `b` in hive `work`, `a`'s turn held
/// open on an episode, and a server serving both seats.
pub struct Hive {
    pub coordinator: Coordinator,
    pub server: HiveToolServer,
    pub turn: TurnRequest,
    pub finish: Arc<Notify>,
    pub a: Client,
    pub b: Client,
    pub endpoint: ToolEndpoint,
    pub workspace: tempfile::TempDir,
    drain:
        Option<tokio::task::JoinHandle<tinyhivemind_hives::Result<tinyhivemind_hives::RunReport>>>,
}
impl Hive {
    /// Start with `services` adjusted by `configure` and seat `a` by `seat`.
    pub async fn start(
        configure: impl FnOnce(Services) -> Services,
        seat: impl FnOnce(SeatTools, ExecContext) -> SeatTools,
    ) -> Self {
        Self::start_with(configure, seat, ServerOptions::default()).await
    }
    pub async fn start_with(
        configure: impl FnOnce(Services) -> Services,
        seat: impl FnOnce(SeatTools, ExecContext) -> SeatTools,
        options: ServerOptions,
    ) -> Self {
        let coordinator = Coordinator::new(
            "runtime".into(),
            Arc::new(MemoryStorage::new()),
            CoordinatorOptions::default(),
        )
        .await
        .unwrap();
        let (started, mut requests) = mpsc::unbounded_channel();
        let finish = Arc::new(Notify::new());
        for id in ["a", "b"] {
            coordinator
                .register_agent(AgentRegistration {
                    agent_id: id.into(),
                    runtime_id: "runtime".into(),
                    runner: Arc::new(Holds {
                        started: started.clone(),
                        finish: finish.clone(),
                    }),
                })
                .await
                .unwrap();
        }
        coordinator
            .create_hive(HiveInfo {
                hive_id: "work".into(),
                name: "Work".into(),
                description: None,
                members: vec!["a".into(), "b".into()],
            })
            .await
            .unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let context = ExecContext {
            package_root: fixtures(),
            workspace: Some(workspace.path().to_path_buf()),
            hive: "work".into(),
            seat: "a".into(),
            ..ExecContext::default()
        };
        let server = HiveToolServer::start(configure(Services::new(coordinator.clone())), options)
            .await
            .unwrap();
        let endpoint = server
            .serve_seat(seat(SeatTools::new("a"), context))
            .unwrap();
        let other = server.serve_seat(SeatTools::new("b")).unwrap();
        coordinator
            .send_as_host(SendMessage {
                message_id: "task".into(),
                sender: String::new(),
                destination: Destination::Hive("work".into()),
                body: "decide".into(),
                thread: None,
                only_for: Vec::new(),
                starters: vec!["a".into()],
            })
            .await
            .unwrap();
        let drain = tokio::spawn({
            let coordinator = coordinator.clone();
            async move { coordinator.run_until_idle().await }
        });
        let turn = requests.recv().await.unwrap();
        // Later turns, such as the conductor's nudges, are not held.
        drop(requests);
        Self {
            a: Client::of(&server, &endpoint),
            b: Client::of(&server, &other),
            coordinator,
            server,
            turn,
            finish,
            endpoint,
            workspace,
            drain: Some(drain),
        }
    }
    /// Let `a`'s turn end and the coordinator go idle.
    pub async fn settle(&mut self) {
        self.finish.notify_one();
        if let Some(drain) = self.drain.take() {
            drain.await.unwrap().unwrap();
        }
    }
    /// Messages `sender` wrote with exactly `body`, in the whole transcript.
    pub fn written(&self, sender: &str, body: &str) -> usize {
        let rows = self.coordinator.read_transcript(None).unwrap();
        rows.iter()
            .filter(|row| row.sender == sender && row.body == body)
            .count()
    }
    /// The episode `a`'s turn works on.
    pub fn episode(&self) -> String {
        self.turn.episode.as_ref().unwrap().episode_id.clone()
    }
    /// `a`'s running turn id.
    pub fn turn_id(&self) -> String {
        self.turn.turn_id.clone()
    }
    /// Lines `file` holds in the seat's workspace.
    pub fn lines(&self, file: &str) -> usize {
        std::fs::read_to_string(self.workspace.path().join(file))
            .unwrap_or_default()
            .lines()
            .count()
    }
}

/// The fixture package root.
pub fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// An executable tool `id` running `tools/<script>.sh`, with `args` as its
/// argument schema.
pub fn tool(
    id: &str,
    script: &str,
    side_effects: &str,
    args: Value,
) -> tinyhivemind_lang::ExecTool {
    serde_json::from_value(json!({
        "id": id,
        "description": format!("fixture {script}"),
        "command": [format!("tools/{script}.sh")],
        "args": args,
        "side_effects": side_effects,
    }))
    .expect("a valid tool")
}

/// A hook `id` on `event` for tools `matcher`, running `tools/<script>.sh`.
pub fn hook(id: &str, event: &str, matcher: &str, script: &str) -> tinyhivemind_lang::LoweredHook {
    tinyhivemind_lang::LoweredHook {
        hook: serde_json::from_value(json!({
            "id": id, "event": event, "matcher": matcher, "run": script,
        }))
        .expect("a valid hook"),
        tool: tool(script, script, "write", json!({"type": "object"})),
    }
}
