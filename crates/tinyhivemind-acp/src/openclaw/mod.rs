//! `OpenClaw`: its ACP bridge as a seat's agent, and the Gateway its seats
//! share.
//!
//! `openclaw acp` is a bridge to an `OpenClaw` Gateway, and it refuses MCP
//! servers per session. So a seat's tool server is configured in the
//! Gateway instead: one Gateway agent per seat, with that seat's server and
//! bearer, allowed that server's tools alone. [`GatewayPlan`] writes that
//! config, [`Gateway`] supervises the Gateway process, and
//! [`Agent::openclaw`] runs a seat's bridge against it as that seat's agent.
//!
//! Every seat of a Gateway is in its plan when it starts; a seat added later
//! needs a new Gateway. The bridge reports no per-turn token usage, so an
//! `OpenClaw` seat's turns carry none.
use crate::agent::{Agent, NO_KEY, base_env};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::fmt;
use std::fs::Permissions;
use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tinyhivemind_host::harness::{self, ToolEndpoint};
use tinyhivemind_host::process::{Instance, InstanceDir, Launch, StopPolicy};
use tinyhivemind_lang::HarnessKind;

/// The tool groups a Gateway agent is denied: everything but the host's
/// tools, as in `docs/experiments/2026-11-25-process-harnesses`.
const DENY: [&str; 13] = [
    "group:runtime",
    "group:fs",
    "group:web",
    "group:ui",
    "group:automation",
    "group:messaging",
    "group:nodes",
    "group:agents",
    "group:media",
    "group:memory",
    "group:sessions",
    "group:openclaw",
    "transcripts",
];

/// The environment the Gateway and the bridge share, beyond
/// [`base_env`]: their state and config under `{state}`, and nothing that
/// reaches out on its own.
const OPENCLAW_ENV: [(&str, &str); 6] = [
    ("OPENCLAW_STATE_DIR", "{state}/openclaw"),
    ("OPENCLAW_CONFIG_PATH", "{state}/openclaw.json"),
    ("OPENCLAW_SKIP_CHANNELS", "1"),
    ("OPENCLAW_DISABLE_BONJOUR", "1"),
    ("OPENCLAW_NO_RESPAWN", "1"),
    ("OPENCLAW_EXEC_SHELL_SNAPSHOT", "0"),
];

/// One seat of a Gateway: its id and its tools.
#[derive(Clone, Debug)]
pub struct GatewaySeat {
    /// The seat's id, which names its Gateway agent and session.
    pub id: String,
    /// The host's tool server for this seat.
    pub tools: ToolEndpoint,
}

/// How to run one Gateway, and the seats it serves.
#[derive(Clone)]
pub struct GatewayPlan {
    /// `node`, an absolute path.
    pub node: PathBuf,
    /// `OpenClaw`'s `openclaw.mjs`, an absolute path.
    pub openclaw: PathBuf,
    /// The Gateway's private directory, absolute, whose parent exists.
    pub directory: PathBuf,
    /// The loopback port it listens on.
    pub port: u16,
    /// The token its bridges present.
    pub token: String,
    /// The Chat Completions API root of every seat's model.
    pub endpoint: String,
    /// The model every seat runs.
    pub model: String,
    /// Its seats.
    pub seats: Vec<GatewaySeat>,
}

impl fmt::Debug for GatewayPlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GatewayPlan")
            .field("node", &self.node)
            .field("openclaw", &self.openclaw)
            .field("directory", &self.directory)
            .field("port", &self.port)
            .field("token", &"<redacted>")
            .field("endpoint", &self.endpoint)
            .field("model", &self.model)
            .field("seats", &self.seats)
            .finish()
    }
}

/// The name a seat's tool server has in the Gateway, which its tools are
/// named after: one per seat, since every seat has its own bearer.
fn server_key(server: &str, seat: &str) -> String {
    format!("{server}_{seat}")
}

impl GatewayPlan {
    /// The WebSocket URL a bridge reaches the Gateway at.
    #[must_use]
    pub fn url(&self) -> String {
        format!("ws://127.0.0.1:{}", self.port)
    }

    /// The file holding [`GatewayPlan::token`], written when the Gateway
    /// starts, which a bridge reads rather than taking the token as an
    /// argument every local user can see.
    #[must_use]
    pub fn token_file(&self) -> PathBuf {
        self.state().join("token")
    }

    fn state(&self) -> PathBuf {
        self.directory.join("state")
    }

    /// The Gateway's config: the seat's model as its only provider; one
    /// agent per seat, with its own workspace and allowed only its own tool
    /// server; and tool search, code mode and agent-to-agent calls off.
    #[must_use]
    pub fn config(&self) -> Value {
        let state = self.state();
        let mut agents = BTreeMap::new();
        let mut servers = BTreeMap::new();
        for seat in &self.seats {
            let key = server_key(&seat.tools.server, &seat.id);
            let workspace = state.join("workspaces").join(&seat.id);
            agents.insert(
                seat.id.clone(),
                json!({"workspace": workspace, "tools": {"allow": [format!("{key}__*")], "deny": DENY}}),
            );
            servers.insert(
                key,
                json!({"url": seat.tools.url, "transport": "streamable-http",
                    "headers": {"Authorization": format!("Bearer {}", seat.tools.bearer())}}),
            );
        }
        let mut agents = json!({
            "defaults": {"model": {"primary": format!("thm/{}", self.model)}, "skipBootstrap": true,
                "contextInjection": "never", "heartbeat": {"every": "0m"}},
            "entries": agents,
        });
        if self.seats.len() > 1 {
            agents["ownership"] = "explicit".into();
        }
        json!({
            "gateway": {"mode": "local", "bind": "loopback", "port": self.port,
                "auth": {"mode": "token", "token": self.token}},
            "update": {"checkOnStart": false},
            "models": {"mode": "replace", "catalogRefresh": {"enabled": false},
                "providers": {"thm": {"baseUrl": self.endpoint, "apiKey": NO_KEY, "api": "openai-completions",
                    "models": [{"id": self.model, "name": self.model, "contextWindow": 200_000, "maxTokens": 8192}]}}},
            "agents": agents,
            "mcp": {"servers": servers},
            "logging": {"file": state.join("openclaw.log")},
            "tools": {"toolSearch": false, "codeMode": false, "agentToAgent": {"enabled": false},
                "sessions": {"visibility": "self"}, "deny": DENY},
        })
    }
}

/// Whether the Gateway on `port` answers `GET /readyz` with 200: usable,
/// its startup work settled. Listening is not enough: a bridge that connects
/// before then can be turned away and exit.
async fn is_ready(port: u16) -> bool {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let Ok(mut stream) = tokio::net::TcpStream::connect(("127.0.0.1", port)).await else {
        return false;
    };
    let request = "GET /readyz HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    if stream.write_all(request.as_bytes()).await.is_err() {
        return false;
    }
    let mut status = [0_u8; 12];
    stream.read_exact(&mut status).await.is_ok() && status.ends_with(b" 200")
}

/// A running Gateway, stopped with its whole process tree when dropped.
#[derive(Debug)]
pub struct Gateway {
    instance: Instance,
}

impl Gateway {
    /// Write `plan`'s config and token, start the Gateway, and wait at most
    /// `ready` for it to be usable: `GET /readyz` answered with 200.
    ///
    /// # Errors
    /// A [`harness::Error`]: a file could not be written, the process could
    /// not be started, or it exited or was not usable within `ready`.
    pub async fn start(plan: &GatewayPlan, ready: Duration) -> Result<Self, harness::Error> {
        let dir = InstanceDir::create(&plan.directory).map_err(harness::Error::Process)?;
        let state = dir.state().to_path_buf();
        let write = |path: PathBuf, content: String| {
            std::fs::write(&path, content)
                .map_err(|error| harness::Error::Harness(format!("{}: {error}", path.display())))
        };
        // Private whatever the umask: OpenClaw refuses a temporary directory
        // under a group- or world-writable one, and never starts a turn.
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        let workspaces = plan
            .seats
            .iter()
            .map(|seat| state.join("workspaces").join(&seat.id));
        for path in [state.join("openclaw"), state.join("home")]
            .into_iter()
            .chain(workspaces)
        {
            builder
                .create(&path)
                .and_then(|()| std::fs::set_permissions(&path, Permissions::from_mode(0o700)))
                .map_err(|error| harness::Error::Harness(format!("{}: {error}", path.display())))?;
        }
        write(plan.token_file(), plan.token.clone())?;
        write(state.join("openclaw.json"), plan.config().to_string())?;
        let fill = |value: &str| {
            let program_dir = plan.node.parent().unwrap_or(Path::new("/"));
            value
                .replace("{state}", &state.to_string_lossy())
                .replace("{home}", &state.join("home").to_string_lossy())
                .replace("{program_dir}", &program_dir.to_string_lossy())
        };
        let mut launch = Launch::new(&plan.node, dir)
            .arg(&plan.openclaw)
            .arg("gateway");
        for (name, value) in base_env()
            .iter()
            .map(|(n, v)| (n.as_str(), v.as_str()))
            .chain(OPENCLAW_ENV)
        {
            launch = launch.env(name, fill(value));
        }
        let instance = Instance::start(launch)
            .await
            .map_err(harness::Error::Process)?;
        let port = plan.port;
        let usable = async move {
            while !is_ready(port).await {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Ok::<(), String>(())
        };
        if let Err(error) = instance.ready(usable, ready).await {
            let _ = instance.stop(async {}, StopPolicy::default()).await;
            return Err(harness::Error::Process(error));
        }
        Ok(Self { instance })
    }

    /// The Gateway's process id.
    #[must_use]
    pub fn pid(&self) -> u32 {
        self.instance.pid()
    }

    /// Stop the Gateway and every process it started.
    ///
    /// # Errors
    /// A [`harness::Error::Process`] if its processes could not be shown
    /// gone.
    pub async fn stop(&self) -> Result<(), harness::Error> {
        self.instance
            .stop(async {}, StopPolicy::default())
            .await
            .map(|_| ())
            .map_err(harness::Error::Process)
    }
}

impl Agent {
    /// `OpenClaw`'s ACP bridge, `openclaw acp`, run by `node` from `openclaw`
    /// (its `openclaw.mjs`) against the Gateway `gateway` plans: the seat's
    /// session is its Gateway agent's, and its tools are the ones the
    /// Gateway was given for it, so none are passed per session.
    #[must_use]
    pub fn openclaw(
        node: impl Into<PathBuf>,
        openclaw: impl Into<PathBuf>,
        version: impl Into<String>,
        gateway: &GatewayPlan,
    ) -> Self {
        let mut env = base_env();
        env.extend(OPENCLAW_ENV.map(|(name, value)| (name.to_owned(), value.to_owned())));
        let token_file = gateway.token_file().to_string_lossy().into_owned();
        let args = [
            openclaw.into().to_string_lossy().into_owned(),
            "acp".into(),
            "--url".into(),
            gateway.url(),
            "--token-file".into(),
            token_file,
            "--session".into(),
            "agent:{seat}:acp".into(),
            "--no-prefix-cwd".into(),
        ];
        Self {
            kind: HarnessKind::OpenClaw,
            program: node.into(),
            args: args.into(),
            version: version.into(),
            env,
            files: Vec::new(),
            session_meta: None,
            mode: None,
            session_mcp: false,
            tool_name: "{server}_{seat}__{tool}".into(),
            vars: BTreeMap::new(),
        }
    }
}

#[cfg(test)]
mod test;
