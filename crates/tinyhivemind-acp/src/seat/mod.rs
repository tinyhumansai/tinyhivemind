//! One seat: its agent process, its session, and the turn it may be running.
//!
//! An instance lives in the seat's directory, made private by
//! [`InstanceDir`]. Its state directory holds `home/` (the process's
//! `HOME`), `cwd/` (its working directory and the session's `cwd`) and the
//! descriptor's files. The state survives a restart, so a session the agent
//! stored there can be loaded again.
mod turn;

use crate::agent::{Agent, Values};
use crate::capability::SeatModel;
use crate::connection::Connection;
use crate::error::{Error, Result};
use crate::harness::Options;
use crate::protocol;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::os::unix::fs::DirBuilderExt as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tinyhivemind_hives::{AgentRunner, TurnFuture, TurnOutcome, TurnRequest};
use tinyhivemind_host::harness::{
    self, CancelFuture, CancellableRunner, Event, Expected, Observed, SeatPlan, Started,
    ToolEndpoint,
};
use tinyhivemind_host::process::{Instance, InstanceDir, Launch};
use tokio::sync::{Mutex as AsyncMutex, watch};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A started, verified agent process.
#[derive(Debug)]
pub(crate) struct Live {
    pub(crate) instance: Instance,
    pub(crate) connection: Connection,
    pub(crate) started: Started,
    /// Whether the agent can `session/load` a session.
    load_session: bool,
    /// Sessions opened or loaded by this process.
    sessions: Mutex<HashSet<String>>,
    /// Of those, the ones in the descriptor's mode.
    moded: Mutex<HashSet<String>>,
}

/// The turn running now: its session, and the signal that it is cancelled.
#[derive(Debug)]
struct Active {
    session: String,
    cancelled: watch::Sender<bool>,
}

impl Active {
    /// Tell the turn it is cancelled; its session.
    fn cancel(&self) -> String {
        self.cancelled.send_replace(true);
        self.session.clone()
    }
}

/// A seat and its process, if one is running.
#[derive(Debug)]
pub(crate) struct Seat {
    agent: Agent,
    options: Options,
    plan: SeatPlan,
    tools: ToolEndpoint,
    values: Values,
    live: AsyncMutex<Option<Arc<Live>>>,
    /// The turn running now, for a cancel.
    active: Mutex<Option<Active>>,
    /// The seat's one session, once it has one.
    session: Mutex<Option<String>>,
    closed: watch::Sender<bool>,
}

/// The state directory [`InstanceDir`] keeps under a seat's directory.
fn state_of(directory: &Path) -> PathBuf {
    directory.join("state")
}

/// The template values of one seat.
fn values(agent: &Agent, plan: &SeatPlan, tools: &ToolEndpoint, model: &SeatModel) -> Values {
    let state = state_of(&plan.directory);
    let path = |path: &Path| path.to_string_lossy().into_owned();
    let program_dir = agent.program.parent().map(path).unwrap_or_default();
    let mut values = BTreeMap::from([
        ("state".to_owned(), path(&state)),
        ("home".to_owned(), path(&state.join("home"))),
        ("endpoint".to_owned(), model.endpoint.clone()),
        ("model".to_owned(), model.model.clone()),
        ("server".to_owned(), tools.server.clone()),
        ("program_dir".to_owned(), program_dir),
    ]);
    values.extend(agent.vars.clone());
    let allowed_tools = tools
        .tools
        .iter()
        .map(|tool| {
            agent
                .tool_name
                .replace("{server}", &tools.server)
                .replace("{tool}", tool)
        })
        .collect();
    Values {
        values,
        allowed_tools,
    }
}

impl Seat {
    pub(crate) fn new(
        agent: Agent,
        options: Options,
        plan: SeatPlan,
        tools: ToolEndpoint,
        model: &SeatModel,
    ) -> Self {
        let values = values(&agent, &plan, &tools, model);
        Self {
            agent,
            options,
            plan,
            tools,
            values,
            live: AsyncMutex::new(None),
            active: Mutex::new(None),
            session: Mutex::new(None),
            closed: watch::channel(false).0,
        }
    }

    fn cwd(&self) -> PathBuf {
        state_of(&self.plan.directory).join("cwd")
    }

    /// Start a process if none is running, reporting it.
    pub(crate) async fn start(&self) -> Result<Arc<Live>> {
        let mut live = self.live.lock().await;
        if *self.closed.borrow() {
            return Err(Error::Closed);
        }
        if let Some(running) = live.as_ref().filter(|live| live.instance.exit().is_none()) {
            return Ok(running.clone());
        }
        // Kept until it is proved stopped, so a failed stop can be retried.
        if let Some(dead) = live.clone() {
            dead.instance.stop(async {}, self.options.stop).await?;
            *live = None;
        }
        let started = Arc::new(self.launch().await?);
        if *self.closed.borrow() {
            started.instance.stop(async {}, self.options.stop).await?;
            return Err(Error::Closed);
        }
        let event = Event::Started(started.started.clone());
        self.plan.observer.event(&self.plan.seat.id, event);
        *live = Some(started.clone());
        Ok(started)
    }

    /// Write the descriptor's files, start the agent and verify it; or leave
    /// nothing running.
    async fn launch(&self) -> Result<Live> {
        let dir = InstanceDir::create(&self.plan.directory)?;
        let state = dir.state().to_path_buf();
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        for sub in ["home", "cwd"] {
            builder
                .create(state.join(sub))
                .map_err(Error::io("the instance directories"))?;
        }
        for (path, template) in &self.agent.files {
            let content = self.values.fill_file(path, template);
            let path = state.join(path);
            if let Some(parent) = path.parent() {
                builder
                    .create(parent)
                    .map_err(Error::io("an agent file's directory"))?;
            }
            fs::write(&path, content).map_err(Error::io("an agent file"))?;
        }
        let mut launch = Launch::new(&self.agent.program, dir.clone())
            .current_dir(self.cwd())
            .json_lines();
        for argument in &self.agent.args {
            launch = launch.arg(self.values.fill(argument));
        }
        for (name, value) in &self.agent.env {
            launch = launch.env(name, self.values.fill(value));
        }
        let mut instance = Instance::start(launch).await?;
        let Some(lines) = instance.take_lines() else {
            let stopped = instance.stop(async {}, self.options.stop).await;
            return Err(stopped.map_or_else(Error::from, |_| Error::Closed));
        };
        let connection = Connection::new(lines);
        let initialize = connection.call("initialize", protocol::initialize());
        let init = match instance.ready(initialize, self.options.ready).await {
            Ok(init) => init,
            Err(error) => {
                let _ = instance.stop(async {}, self.options.stop).await;
                return Err(error.into());
            }
        };
        let accepted = match protocol::accept(&init) {
            Ok(accepted) => accepted,
            Err(reason) => {
                let _ = instance.stop(async {}, self.options.stop).await;
                return Err(Error::Unsupported(reason));
            }
        };
        let pid = instance.pid();
        let version = protocol::agent_version(&init).unwrap_or_default();
        let names = self.values.allowed_tools.clone();
        let expected = Expected::new(pid, &self.agent.version).tools(names.clone());
        // ACP shows no tool list; these are the names the descriptor allows.
        let observed = Observed::new(pid, version).tools(names);
        match expected.verify(&observed) {
            Ok(started) => Ok(Live {
                instance,
                connection,
                started,
                load_session: accepted.load_session,
                sessions: Mutex::default(),
                moded: Mutex::default(),
            }),
            Err(mismatch) => {
                let _ = instance.stop(async {}, self.options.stop).await;
                Err(mismatch.into())
            }
        }
    }

    /// The `session/new` and `session/load` parameters.
    fn session_params(&self) -> Value {
        let mut params = json!({
            "cwd": self.cwd(),
            "mcpServers": [protocol::mcp_server(&self.tools)],
        });
        if let Some(meta) = &self.agent.session_meta {
            params["_meta"] = self.values.fill_json(meta);
        }
        params
    }

    /// The session for a turn that asks for `requested`: the one this process
    /// has, one it loads, or a new one for the seat's first turn; in the
    /// descriptor's mode.
    ///
    /// A session is the seat's once it is opened, before its mode is set, so
    /// a failed `session/set_mode` is retried on that session next turn
    /// rather than leaving it behind for a new one.
    async fn session(&self, live: &Live, requested: Option<String>) -> Result<String> {
        let requested = requested.or_else(|| lock(&self.session).clone());
        let open = requested
            .as_ref()
            .filter(|session| lock(&live.sessions).contains(*session))
            .cloned();
        let session = match (open, requested) {
            (Some(session), _) => session,
            (None, Some(session)) => self.load(live, session).await?,
            (None, None) => {
                let params = self.session_params();
                let created = live.connection.call("session/new", params).await?;
                created["sessionId"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .map(str::to_owned)
                    .ok_or_else(|| Error::Broken("session/new returned no sessionId".into()))?
            }
        };
        lock(&live.sessions).insert(session.clone());
        *lock(&self.session) = Some(session.clone());
        if let Some(mode) = &self.agent.mode
            && !lock(&live.moded).contains(&session)
        {
            let params = json!({"sessionId": session, "modeId": mode});
            live.connection.call("session/set_mode", params).await?;
            lock(&live.moded).insert(session.clone());
        }
        Ok(session)
    }

    /// Load `session` into this process, which must be able to.
    async fn load(&self, live: &Live, session: String) -> Result<String> {
        if !live.load_session {
            return Err(Error::Resume {
                session,
                reason: "the agent cannot load a session".into(),
            });
        }
        let mut params = self.session_params();
        params["sessionId"] = session.clone().into();
        let loaded = live.connection.call("session/load", params).await;
        loaded.map_err(|error| Error::Resume {
            session: session.clone(),
            reason: error.to_string(),
        })?;
        Ok(session)
    }

    async fn run(&self, request: TurnRequest) -> Result<TurnOutcome> {
        if *self.closed.borrow() {
            return Err(Error::Closed);
        }
        let live = self.start().await?;
        let session = self.session(&live, request.session_id.clone()).await?;
        let text = tinyhivemind_host::turn::render(&request)
            .map_err(|error| Error::Render(error.to_string()))?;
        let (cancelled, cancel) = watch::channel(false);
        *lock(&self.active) = Some(Active {
            session: session.clone(),
            cancelled,
        });
        let turn = turn::Turn {
            live: &live,
            seat: &self.plan.seat.id,
            server: &self.tools.server,
            id: &request.turn_id,
            session: &session,
            plan: &self.plan,
            approval: self.options.approval,
            cancelled: cancel,
        };
        let ended = turn.run(&text).await;
        lock(&self.active).take();
        let ended = ended?;
        if let Some(usage) = ended.usage {
            let event = Event::Usage {
                turn_id: request.turn_id.clone(),
                usage,
            };
            self.plan.observer.event(&self.plan.seat.id, event);
        }
        Ok(TurnOutcome {
            session_id: session,
            reply: Some(ended.reply).filter(|reply| !reply.is_empty()),
            disposition: ended.disposition,
        })
    }

    /// Cancel the running turn and see its prompt answered, or stop the
    /// process.
    async fn cancel(&self) -> Result<()> {
        let Some(session) = lock(&self.active).as_ref().map(Active::cancel) else {
            return Ok(());
        };
        let mut live = self.live.lock().await;
        let Some(running) = live.clone() else {
            return Ok(());
        };
        if running.instance.exit().is_none() {
            let params = json!({"sessionId": session});
            let _ = running.connection.notify("session/cancel", params).await;
            let deadline = tokio::time::Instant::now() + self.options.cancel;
            while lock(&self.active).is_some() && tokio::time::Instant::now() < deadline {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            if lock(&self.active).is_none() {
                return Ok(());
            }
        }
        // It did not show the turn stopped: nothing of it may run on.
        running.instance.stop(async {}, self.options.stop).await?;
        *live = None;
        lock(&self.active).take();
        Ok(())
    }

    /// Cancel, then stop the process and prove it gone. The process is kept
    /// until then, so a failed stop can be retried.
    pub(crate) async fn stop(&self) -> Result<()> {
        self.closed.send_replace(true);
        let mut held = self.live.lock().await;
        let Some(live) = held.clone() else {
            return Ok(());
        };
        let active = lock(&self.active).as_ref().map(Active::cancel);
        let cancel = async {
            if let Some(session) = active {
                let params = json!({"sessionId": session});
                let _ = live.connection.notify("session/cancel", params).await;
            }
        };
        live.instance.stop(cancel, self.options.stop).await?;
        *held = None;
        Ok(())
    }
}

/// The seat as the seat runner sees it.
pub(crate) struct Runner(pub(crate) Arc<Seat>);

impl AgentRunner for Runner {
    fn run(&self, request: TurnRequest) -> TurnFuture {
        let seat = self.0.clone();
        Box::pin(async move { Ok(seat.run(request).await.map_err(harness::Error::from)?) })
    }
}

impl CancellableRunner for Runner {
    fn cancel(&self, _: &str) -> CancelFuture {
        let seat = self.0.clone();
        Box::pin(async move { Ok(seat.cancel().await?) })
    }
}
