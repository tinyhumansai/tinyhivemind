//! The pinned `openhuman-core` as a subject of the conformance suite: the
//! scripted model in `model.rs`, and the spike fixture's MCP stub, run by
//! Node, for the host's tools.
use crate::model::ScriptedModel;
use crate::seat_support::{kill_all, processes_in, seat};
use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Duration;
use tinyhivemind_host::harness::conformance::{EscapeFuture, ModelCall, ModelStep, Subject, ToolFault};
use tinyhivemind_host::harness::{Harness, SeatPlan, ToolEndpoint};
use tinyhivemind_host::process::StopPolicy;
use tinyhivemind_openhuman_process::{OpenHumanHarness, Options, Program};
use tokio::io::{AsyncBufReadExt as _, BufReader};
use tokio::process::{Child, Command};

/// The pinned version, as `harnesses.lock` records it.
pub(crate) const PINNED: &str = "0.64.15";

/// What a live test needs, or why it cannot run here.
pub(crate) fn requirements() -> Result<(PathBuf, PathBuf), String> {
    let binary = std::env::var_os("THM_OPENHUMAN_CORE")
        .map(PathBuf::from)
        .ok_or("THM_OPENHUMAN_CORE is not set to the pinned openhuman-core")?;
    let node = std::env::var_os("THM_NODE").map(PathBuf::from).or_else(|| {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path).map(|dir| dir.join("node")).find(|node| node.is_file())
    });
    let node = node.ok_or("node is neither THM_NODE nor on PATH, and the fixture needs it")?;
    Ok((binary, node))
}

/// The spike's fixture: a model that ignores its key and an MCP stub that
/// maps `seat-a-token` to seat `a`.
pub(crate) struct Fixture {
    pub(crate) port: u16,
    pub(crate) log: PathBuf,
    _child: Child,
}

impl Fixture {
    pub(crate) async fn start(node: &Path, dir: &Path) -> std::io::Result<Self> {
        let port = std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.port();
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/experiments/2026-11-25-process-harnesses/fixture/server.mjs");
        let log = dir.join("fixture.jsonl");
        let mut child = Command::new(node)
            .arg(script)
            .arg(port.to_string())
            .arg(&log)
            .stdout(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let stdout = child.stdout.take().ok_or(std::io::ErrorKind::BrokenPipe)?;
        let mut lines = BufReader::new(stdout).lines();
        while let Some(line) = lines.next_line().await? {
            if line.trim() == "ready" {
                return Ok(Self { port, log, _child: child });
            }
        }
        Err(std::io::ErrorKind::UnexpectedEof.into())
    }

    pub(crate) fn entries(&self) -> Vec<serde_json::Value> {
        fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }
}

/// Generous: the real binary takes up to a second to start.
pub(crate) fn options() -> Options {
    Options {
        ready: Duration::from_secs(30),
        warm_up: Duration::from_secs(20),
        stop: StopPolicy::default(),
    }
}

pub(crate) struct LiveSubject {
    root: tempfile::TempDir,
    model: ScriptedModel,
    fixture: Fixture,
    fault: AtomicU8,
}

impl LiveSubject {
    pub(crate) async fn new() -> anyhow::Result<Option<Self>> {
        let (binary, node) = match requirements() {
            Ok(found) => found,
            Err(reason) => {
                eprintln!("skipped: {reason}");
                return Ok(None);
            }
        };
        let root = tempfile::tempdir()?;
        let fixture = Fixture::start(&node, root.path()).await?;
        // Corrupts the written config on request, so that the binary falls
        // back to its defaults as it does on bad TOML.
        let script = format!(
            "#!/bin/sh\n[ -e '{}/misconfigured' ] && printf '\\n[[[\\n' >> \"$OPENHUMAN_WORKSPACE/config.toml\"\nexec '{}' \"$@\"\n",
            root.path().display(),
            binary.display()
        );
        let wrapper = root.path().join("openhuman-core");
        fs::write(&wrapper, script)?;
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755))?;
        Ok(Some(Self {
            model: ScriptedModel::start().await?,
            root,
            fixture,
            fault: AtomicU8::new(0),
        }))
    }

    fn instance(&self) -> PathBuf {
        self.root.path().join("instance")
    }
}

impl Subject for LiveSubject {
    fn harness(&self) -> Arc<dyn Harness> {
        let program = Program::new(self.root.path().join("openhuman-core"), PINNED);
        Arc::new(OpenHumanHarness::new(program).with_options(options()))
    }

    fn seat(&self) -> SeatPlan {
        let endpoint = format!("http://127.0.0.1:{}/v1", self.model.port);
        SeatPlan::new(seat(&endpoint), self.instance())
    }

    fn endpoint(&self) -> ToolEndpoint {
        let mut tools = vec!["hivemind_post".to_owned()];
        if self.fault.load(Ordering::Relaxed) == 2 {
            // The stub serves no such tool, so the instance never lists it.
            tools.push("hivemind_absent".to_owned());
        }
        let url = format!("http://127.0.0.1:{}/mcp", self.fixture.port);
        ToolEndpoint::new("hivea", url, "seat-a-token", tools)
    }

    fn script(&self, steps: &[ModelStep]) {
        self.model.script(steps);
    }

    fn model_calls(&self) -> Vec<ModelCall> {
        self.model.calls()
    }

    fn lose_sessions(&self) {
        let workspace = self.instance().join("state/ws/workspace");
        for store in ["session_raw", "sessions", "session_db", "memory/conversations"] {
            let _ = fs::remove_dir_all(workspace.join(store));
        }
    }

    /// The binary shows its model only what the exact allowlist names, so an
    /// extra tool can only come from another config: that is what `Extra`
    /// writes. `Missing` names a tool the stub does not serve.
    fn offer(&self, fault: ToolFault) {
        let (code, misconfigured) = match fault {
            ToolFault::None => (0, false),
            ToolFault::Extra => (1, true),
            ToolFault::Missing => (2, false),
        };
        self.fault.store(code, Ordering::Relaxed);
        let marker = self.root.path().join("misconfigured");
        let _ = if misconfigured { fs::write(marker, "") } else { fs::remove_file(marker) };
    }

    fn misconfigure(&self) {
        let _ = fs::write(self.root.path().join("misconfigured"), "");
    }

    /// Built-in tools are off, so nothing can make this instance spawn.
    fn escapee(&self) -> EscapeFuture {
        Box::pin(async { None })
    }

    fn processes(&self) -> Vec<u32> {
        processes_in(&self.instance())
    }

    fn turn_limit(&self) -> Duration {
        Duration::from_secs(20)
    }

    fn patience(&self) -> Duration {
        Duration::from_secs(90)
    }
}

impl Drop for LiveSubject {
    fn drop(&mut self) {
        kill_all(self.processes());
    }
}
