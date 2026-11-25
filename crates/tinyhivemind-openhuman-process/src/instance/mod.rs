//! Starting one instance: its directories, environment and config, tool
//! discovery, and the verification it must pass.
//!
//! An instance lives in the seat's directory, made private by
//! [`InstanceDir`]. Its state directory holds:
//!
//! | Path | For |
//! | --- | --- |
//! | `ws/` | `OPENHUMAN_WORKSPACE`: `config.toml`, sessions, the keyring |
//! | `home/` | `HOME` |
//! | `action/` | `OPENHUMAN_ACTION_DIR`, and the working directory |
//! | `keyring.key` | the keyring's random master key, mode 0600, kept across starts |
//!
//! A start is two starts. The model-visible name of an MCP tool carries a
//! hash that only the instance knows, so the first start runs with an
//! allowlist that matches nothing, waits out the MCP warm-up and reads the
//! names from `agent_registry_available_tools`; then it is stopped, the exact
//! allowlist is written, and the second start is verified. The core token is
//! new for every start; the port is a free one, and an instance that moved
//! to another fails the pid check. `OPENHUMAN_SANDBOX` is never set.
mod verify;

pub(crate) use verify::{discover, verify};

use crate::capability::SeatModel;
use crate::config::{Input, Written};
use crate::error::{Error, Result};
use crate::harness::{Options, Program};
use crate::rpc::Client;
use std::fs;
use std::io::{Read as _, Write as _};
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tinyhivemind_host::harness::{SeatPlan, Started, ToolEndpoint};
use tinyhivemind_host::process::{Instance, InstanceDir, Launch};

/// The allowlist of the discovery start, which no tool is called.
const NOTHING: &str = "thm_no_tool";
/// The least a turn may run inside the binary, which has its own deadline.
const TURN_FLOOR: u64 = 600;

/// A started, verified instance.
#[derive(Debug)]
pub(crate) struct Live {
    pub(crate) instance: Instance,
    pub(crate) client: Client,
    pub(crate) started: Started,
}

/// The directories of one instance's state.
#[derive(Clone, Debug)]
pub(crate) struct Layout {
    state: PathBuf,
}

impl Layout {
    pub(crate) fn new(state: &Path) -> Self {
        Self {
            state: state.to_path_buf(),
        }
    }
    pub(crate) fn workspace(&self) -> PathBuf {
        self.state.join("ws")
    }
    fn home(&self) -> PathBuf {
        self.state.join("home")
    }
    fn action(&self) -> PathBuf {
        self.state.join("action")
    }
    fn create(&self) -> Result<()> {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true).mode(0o700);
        for dir in [self.workspace(), self.home(), self.action()] {
            builder.create(dir).map_err(Error::io("the instance directories"))?;
        }
        Ok(())
    }

    /// The keyring's master key, made on first use.
    fn master_key(&self) -> Result<String> {
        let path = self.state.join("keyring.key");
        if let Ok(key) = fs::read_to_string(&path) {
            return Ok(key);
        }
        let key = random()?;
        let mut file = fs::File::options()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .map_err(Error::io("the keyring key"))?;
        file.write_all(key.as_bytes()).map_err(Error::io("the keyring key"))?;
        Ok(key)
    }

    fn write_config(&self, written: &Written) -> Result<()> {
        let path = self.workspace().join("config.toml");
        let mut file = fs::File::options()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .map_err(Error::io("the config"))?;
        file.write_all(written.toml().as_bytes()).map_err(Error::io("the config"))
    }
}

/// 256 random bits, as hex.
fn random() -> Result<String> {
    let mut bytes = [0_u8; 32];
    let mut source = fs::File::open("/dev/urandom").map_err(Error::io("/dev/urandom"))?;
    source.read_exact(&mut bytes).map_err(Error::io("/dev/urandom"))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// The child's whole environment, besides what the supervisor adds.
pub(crate) fn environment(layout: &Layout, token: &str, key: &str) -> Vec<(&'static str, String)> {
    let path = |path: PathBuf| path.to_string_lossy().into_owned();
    vec![
        ("PATH", "/usr/bin:/bin".into()),
        ("HOME", path(layout.home())),
        ("LANG", "C.UTF-8".into()),
        ("OPENHUMAN_WORKSPACE", path(layout.workspace())),
        ("OPENHUMAN_ACTION_DIR", path(layout.action())),
        ("OPENHUMAN_TOOL_DISPATCHER", "native".into()),
        ("OPENHUMAN_COMPOSIO_MODE", "disabled".into()),
        // The binary asks for a backend key; this one opens nothing.
        ("OPENHUMAN_BACKEND_API_KEY", "dummy".into()),
        ("OPENHUMAN_KEYRING_BACKEND", "encrypted_file".into()),
        ("OPENHUMAN_KEYRING_MASTER_KEY", key.into()),
        ("OPENHUMAN_CORE_TOKEN", token.into()),
        ("RUST_LOG", "warn".into()),
    ]
}

/// A port nobody is listening on now.
fn free_port() -> Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(Error::io("a free port"))?;
    let address = listener.local_addr().map_err(Error::io("a free port"))?;
    Ok(address.port())
}

/// How long the binary itself lets a turn run: past the seat's own limit,
/// which the seat runner enforces.
fn turn_secs(limit: Option<Duration>) -> u64 {
    let limit = limit.map_or(0, |limit| limit.as_secs().saturating_add(60));
    limit.max(TURN_FLOOR)
}

/// Starts a seat's instances.
pub(crate) struct Launcher<'a> {
    pub(crate) program: &'a Program,
    pub(crate) options: &'a Options,
    pub(crate) plan: &'a SeatPlan,
    pub(crate) tools: &'a ToolEndpoint,
    pub(crate) model: &'a SeatModel,
}

impl Launcher<'_> {
    /// Start, discover, restart and verify; or leave nothing running.
    pub(crate) async fn launch(&self) -> Result<Live> {
        let dir = InstanceDir::create(&self.plan.directory)?;
        let layout = Layout::new(dir.state());
        layout.create()?;
        let key = layout.master_key()?;
        let nothing = [NOTHING.to_owned()];
        let ((probe, client), _) = self.start(&dir, &layout, &key, &nothing).await?;
        let server = &self.tools.server;
        let names = discover(&client, server, &self.tools.tools, self.options.warm_up).await;
        probe.stop(async {}, self.options.stop).await?;
        let names = names?;
        let ((instance, client), written) = self.start(&dir, &layout, &key, &names).await?;
        let version = &self.program.version;
        let pid = instance.pid();
        let warm_up = self.options.warm_up;
        match verify(&client, pid, version, &written, &names, warm_up).await {
            Ok(started) => Ok(Live {
                instance,
                client,
                started,
            }),
            Err(error) => {
                instance.stop(async {}, self.options.stop).await?;
                Err(error)
            }
        }
    }

    /// One healthy instance with `tools` allowed.
    async fn start(
        &self,
        dir: &InstanceDir,
        layout: &Layout,
        key: &str,
        tools: &[String],
    ) -> Result<((Instance, Client), Written)> {
        let written = Written::new(&Input {
            model: self.model,
            seat: &self.plan.seat.id,
            prompt: &self.plan.seat.prompt,
            server: &self.tools.server,
            url: &self.tools.url,
            bearer: self.tools.bearer(),
            tools,
            turn_secs: turn_secs(self.plan.turn_timeout()),
        });
        layout.write_config(&written)?;
        let token = random()?;
        let port = free_port()?;
        let mut launch = Launch::new(&self.program.path, dir.clone()).current_dir(layout.action());
        for argument in ["run", "--headless-api", "--host", "127.0.0.1", "--port"] {
            launch = launch.arg(argument);
        }
        launch = launch.arg(port.to_string());
        for (name, value) in environment(layout, &token, key) {
            launch = launch.env(name, value);
        }
        let instance = Instance::start(launch).await?;
        let client = Client::new(port, &token)?;
        let probe = async {
            while client.health().await.is_err() {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            Ok::<(), String>(())
        };
        if let Err(error) = instance.ready(probe, self.options.ready).await {
            instance.stop(async {}, self.options.stop).await?;
            return Err(error.into());
        }
        Ok(((instance, client), written))
    }
}

#[cfg(test)]
mod test;
