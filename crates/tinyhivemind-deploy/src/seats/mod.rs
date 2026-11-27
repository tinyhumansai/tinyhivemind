//! A hive's seats on their harness processes, started and stopped together.
use crate::gateway;
use crate::{Error, Installs, Result};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tinyhivemind_acp::openclaw::Gateway;
use tinyhivemind_hives::{AgentRegistration, Coordinator};
use tinyhivemind_host::harness::{Harness, SeatPlan, ToolEndpoint};
use tinyhivemind_lang::LoweredSeat;
use tinyhivemind_mcp::{HiveToolServer, SeatTools, ServerOptions, Services};

/// One seat to start: its lowered seat, which names its harness kind and
/// model; its private directory; and the tools it is served.
#[derive(Debug)]
pub struct ProcessSeat {
    /// The seat, as the package lowers it.
    pub seat: LoweredSeat,
    /// The seat's own directory, absolute, whose parent exists.
    pub directory: PathBuf,
    /// What the tool server serves the seat beside the hive tools: its
    /// executable tools, where they run and under which access.
    pub tools: SeatTools,
}

impl ProcessSeat {
    /// `seat` in `directory`, served the hive tools alone.
    #[must_use]
    pub fn new(seat: LoweredSeat, directory: impl Into<PathBuf>) -> Self {
        let tools = SeatTools::new(seat.id.clone());
        Self {
            seat,
            directory: directory.into(),
            tools,
        }
    }

    /// The same seat, served `tools`, which must be for the same seat id.
    #[must_use]
    pub fn with_tools(mut self, tools: SeatTools) -> Self {
        self.tools = tools;
        self
    }
}

/// How a deployment runs: where it keeps the Gateways' directories and Codex
/// catalogs, and each client's settings.
#[derive(Clone, Debug)]
pub struct Deployment {
    /// The deployment's own directory, absolute.
    pub root: PathBuf,
    /// The hive tool server's limits.
    pub server: ServerOptions,
    /// The `OpenHuman` client's deadlines.
    pub openhuman: tinyhivemind_openhuman_process::Options,
    /// The ACP client's deadlines.
    pub acp: tinyhivemind_acp::Options,
    /// How long an `OpenClaw` Gateway may take to become usable.
    pub gateway_ready: Duration,
}

impl Deployment {
    /// A deployment in `root`, with every client's defaults and three
    /// minutes for a Gateway's cold start.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            server: ServerOptions::default(),
            openhuman: tinyhivemind_openhuman_process::Options::default(),
            acp: tinyhivemind_acp::Options::default(),
            gateway_ready: Duration::from_secs(180),
        }
    }
}

/// A hive's seats running on their harnesses, registered with one
/// coordinator. [`ProcessSeats::stop`] is how they stop: dropping this value
/// does not stop a seat the coordinator still holds, whose registered runner
/// keeps it alive.
pub struct ProcessSeats {
    server: Option<HiveToolServer>,
    gateways: BTreeMap<String, Gateway>,
    /// Started seats not yet stopped, in the order they started.
    seats: Vec<(String, Arc<dyn Harness>)>,
}

impl std::fmt::Debug for ProcessSeats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let seats: Vec<&str> = self.seats.iter().map(|(id, _)| id.as_str()).collect();
        f.debug_struct("ProcessSeats")
            .field("gateways", &self.gateways.keys().collect::<Vec<_>>())
            .field("seats", &seats)
            .finish_non_exhaustive()
    }
}

impl ProcessSeats {
    /// Start `seats` and register each with `coordinator`: their tools are
    /// served by one tool server over `services`, whose approver judges
    /// supervised seats' effectful calls; `OpenClaw` seats get their
    /// Gateways; and each seat starts on the harness its kind maps to in
    /// `installs`. Every seat is checked first, so a seat that cannot run
    /// here starts nothing. A failure after that stops whatever had started
    /// and unregisters every seat it registered, so the same seats can be
    /// started again on the same coordinator.
    ///
    /// # Errors
    /// An [`Error`]: a seat's kind has no install, its model no endpoint or
    /// its harness refuses it as declared, `OpenClaw` seats sharing a
    /// Gateway differ in model, the tool server, a Gateway or a seat did not
    /// start, or a seat could not be registered.
    pub async fn start(
        coordinator: &Coordinator,
        services: Services,
        seats: Vec<ProcessSeat>,
        installs: &Installs,
        deployment: &Deployment,
    ) -> Result<Self> {
        for seat in &seats {
            installs.check(&seat.seat)?;
        }
        private_dir(&deployment.root)?;
        let server = HiveToolServer::start(services, deployment.server.clone()).await?;
        let mut running = Self {
            server: Some(server),
            gateways: BTreeMap::new(),
            seats: Vec::new(),
        };
        let mut registered = Vec::new();
        match running
            .launch(coordinator, seats, installs, deployment, &mut registered)
            .await
        {
            Ok(()) => Ok(running),
            Err(error) => {
                let _ = running.stop().await;
                // A stopped seat's runner would stay registered, and a retry's
                // new runner for the same id would conflict with it.
                for id in registered {
                    let _ = coordinator.unregister_agent(&id).await;
                }
                Err(error)
            }
        }
    }

    /// Serve, then start the Gateways, then each seat, adding each seat
    /// registered with `coordinator` to `registered`.
    async fn launch(
        &mut self,
        coordinator: &Coordinator,
        seats: Vec<ProcessSeat>,
        installs: &Installs,
        deployment: &Deployment,
        registered: &mut Vec<String>,
    ) -> Result<()> {
        let server = self
            .server
            .as_ref()
            .ok_or_else(|| tinyhivemind_mcp::Error::Stopped("the tool server".into()))?;
        let mut endpoints: Vec<ToolEndpoint> = Vec::new();
        for seat in &seats {
            endpoints.push(server.serve_seat(seat.tools.clone())?);
        }
        let lowered: Vec<_> = seats
            .iter()
            .map(|seat| &seat.seat)
            .zip(endpoints.iter())
            .collect();
        let plans = gateway::plan(&lowered, installs, &deployment.root)?;
        for (name, plan) in &plans {
            let gateway = Gateway::start(plan, deployment.gateway_ready)
                .await
                .map_err(|source| Error::Gateway {
                    gateway: name.clone(),
                    source,
                })?;
            self.gateways.insert(name.clone(), gateway);
        }
        for (seat, tools) in seats.into_iter().zip(endpoints) {
            let id = seat.seat.id.clone();
            private_dir(&seat.directory)?;
            let catalog = deployment.root.join(format!("{id}-codex-catalog.json"));
            let gateway = gateway::gateway_of(&seat.seat).and_then(|name| plans.get(&name));
            let harness = installs.harness(&seat.seat, deployment, &catalog, gateway)?;
            let plan = SeatPlan::new(seat.seat, seat.directory);
            let runner = harness
                .start(plan, tools)
                .await
                .map_err(|source| Error::Start {
                    seat: id.clone(),
                    source,
                })?;
            self.seats.push((id.clone(), harness));
            coordinator
                .register_agent(AgentRegistration {
                    agent_id: id.clone(),
                    runtime_id: coordinator.runtime_id().into(),
                    runner,
                })
                .await
                .map_err(|source| Error::Register {
                    seat: id.clone(),
                    source,
                })?;
            registered.push(id);
        }
        Ok(())
    }

    /// The ids of the started seats, in the order they started.
    #[must_use]
    pub fn seats(&self) -> Vec<&str> {
        self.seats.iter().map(|(id, _)| id.as_str()).collect()
    }

    /// Stop every seat, then the Gateways, then the tool server. Every one is
    /// stopped even when an earlier one fails; the first failure is returned.
    /// A seat is forgotten only once it has stopped, so a seat whose stop
    /// failed, or was cut short, is stopped again by the next call.
    ///
    /// # Errors
    /// The first [`Error::Stop`], [`Error::Gateway`] or [`Error::Server`].
    pub async fn stop(&mut self) -> Result<()> {
        let mut first: Option<Error> = None;
        let mut at = 0;
        while let Some((id, harness)) = self.seats.get(at).cloned() {
            match harness.stop(&id).await {
                Ok(()) => {
                    self.seats.remove(at);
                }
                Err(source) => {
                    first.get_or_insert(Error::Stop { seat: id, source });
                    at += 1;
                }
            }
        }
        for (name, gateway) in std::mem::take(&mut self.gateways) {
            if let Err(source) = gateway.stop().await {
                first.get_or_insert(Error::Gateway {
                    gateway: name,
                    source,
                });
            }
        }
        if let Some(server) = self.server.take()
            && let Err(error) = server.shutdown().await
        {
            first.get_or_insert(Error::Server(error));
        }
        first.map_or(Ok(()), Err)
    }
}

/// Make `path` and any missing parent with mode 0700, whatever the umask:
/// `OpenClaw` refuses a temporary directory under a group- or
/// world-writable one, and the agents' state is the seat's own.
/// A directory that already exists is made 0700 too; its parents are left
/// as they are.
fn private_dir(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .and_then(|()| std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)))
        .map_err(Error::io(path.display().to_string()))
}

#[cfg(test)]
mod test;
