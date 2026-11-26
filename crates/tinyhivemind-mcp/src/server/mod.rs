//! The listener: one server per host, every seat behind its own bearer.
//!
//! [`HiveToolServer::start`] binds a random port of 127.0.0.1 and serves
//! HTTP/1.1 at `/mcp`. Each [`HiveToolServer::serve_seat`] draws a fresh
//! bearer and returns the seat's [`ToolEndpoint`]. Calls in flight are
//! bounded across seats and per seat, and a call keeps its place until it
//! finishes, even when its client hung up; connections are not bounded, but
//! an idle one is closed after the header timeout. [`HiveToolServer::shutdown`] stops accepting, lets the
//! open requests finish within a grace period, and returns.
mod http;
mod rpc;

use crate::seat::{Bearer, SeatTools};
use crate::tools::{Ledger, SeatState};
use crate::{Error, Result};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::server::graceful::GracefulShutdown;
use std::fmt;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, PoisonError, RwLock};
use std::time::Duration;
use tinyhivemind_exec::{EnvSecrets, Secrets};
use tinyhivemind_hives::Coordinator;
use tinyhivemind_host::harness::{Approver, ToolEndpoint};
use tinyhivemind_host::policy::{AgentCreator, Management, ManagementAuthorizer, SendAuthorizer};
use tinyhivemind_host::tools::Belt;
use tokio::net::TcpListener;
use tokio::sync::{Semaphore, watch};
use tokio::task::JoinHandle;

/// The MCP protocol revisions served, newest first. In the harness spike
/// `Codex` 0.171 asked for `2025-06-18`, and `OpenHuman` 0.64.15 and
/// `OpenClaw` for `2025-11-25`; an initialize asking for anything else is
/// answered with the newest.
pub const PROTOCOL_VERSIONS: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];

/// What the server dispatches through: the coordinator and the host's ports.
#[derive(Clone)]
pub struct Services {
    pub(crate) coordinator: Coordinator,
    send_policy: Option<Arc<dyn SendAuthorizer>>,
    management: Option<(Arc<dyn ManagementAuthorizer>, Arc<dyn AgentCreator>)>,
    pub(crate) approver: Option<Arc<dyn Approver>>,
    pub(crate) secrets: Arc<dyn Secrets>,
}

impl Services {
    /// Dispatch to `coordinator`, admitting every send, managing nothing,
    /// approving nothing, and resolving secrets from this process's
    /// environment.
    #[must_use]
    pub fn new(coordinator: Coordinator) -> Self {
        Self {
            coordinator,
            send_policy: None,
            management: None,
            approver: None,
            secrets: Arc::new(EnvSecrets),
        }
    }

    /// Gate the outbound hive tools behind `policy`.
    #[must_use]
    pub fn with_send_policy(mut self, policy: Arc<dyn SendAuthorizer>) -> Self {
        self.send_policy = Some(policy);
        self
    }

    /// Serve the management tools, authorized by `authorizer`, agents built
    /// by `creator`.
    #[must_use]
    pub fn with_management(
        mut self,
        authorizer: Arc<dyn ManagementAuthorizer>,
        creator: Arc<dyn AgentCreator>,
    ) -> Self {
        self.management = Some((authorizer, creator));
        self
    }

    /// Ask `approver` about effectful executable tools of supervised seats.
    /// Without one, every such call is refused.
    #[must_use]
    pub fn with_approver(mut self, approver: Arc<dyn Approver>) -> Self {
        self.approver = Some(approver);
        self
    }

    /// Resolve the `env` references of tools and hooks through `secrets`.
    #[must_use]
    pub fn with_secrets(mut self, secrets: Arc<dyn Secrets>) -> Self {
        self.secrets = secrets;
        self
    }

    /// The belt every hive tool call goes through.
    pub(crate) fn belt(&self) -> Belt<'_> {
        let mut belt = Belt::new(&self.coordinator);
        if let Some(policy) = &self.send_policy {
            belt = belt.with_send_policy(policy.as_ref());
        }
        if let Some((authorizer, creator)) = &self.management {
            belt = belt.with_management(Management {
                authorizer: authorizer.as_ref(),
                creator: creator.as_ref(),
            });
        }
        belt
    }
}

impl fmt::Debug for Services {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Services")
            .field("coordinator", &self.coordinator)
            .field("send_policy", &self.send_policy.is_some())
            .field("management", &self.management.is_some())
            .field("approver", &self.approver.is_some())
            .finish_non_exhaustive()
    }
}

/// Bounds and names of a server.
#[derive(Clone, Debug)]
pub struct ServerOptions {
    /// The server's name, which harnesses put in its tools' names.
    pub name: String,
    /// Largest request body accepted; a larger one gets 413.
    pub max_body_bytes: usize,
    /// Most tool calls running at once, across seats; nonzero. A call holds
    /// its place until it finishes, even when its client has hung up.
    pub max_calls: usize,
    /// Most tool calls one seat may have running at once; nonzero.
    pub max_calls_per_seat: usize,
    /// Most call records one seat's running turn may hold.
    pub max_records: usize,
    /// Most bytes of arguments and results those records may hold.
    pub max_record_bytes: usize,
    /// Longest a connection may take to send a request's headers, which is
    /// also how long an idle or unauthenticated connection is kept.
    pub header_timeout: Duration,
    /// Longest a request may take to send its body.
    pub body_timeout: Duration,
    /// Longest shutdown waits for open requests.
    pub shutdown_grace: Duration,
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            name: "hivemind".into(),
            max_body_bytes: 1 << 20,
            max_calls: 32,
            max_calls_per_seat: 4,
            max_records: 256,
            max_record_bytes: 1 << 20,
            header_timeout: Duration::from_secs(5),
            body_timeout: Duration::from_secs(30),
            shutdown_grace: Duration::from_secs(5),
        }
    }
}

/// What every connection shares.
#[derive(Debug)]
pub(crate) struct Shared {
    pub(crate) services: Services,
    pub(crate) options: ServerOptions,
    pub(crate) port: u16,
    seats: RwLock<Vec<Arc<SeatState>>>,
    pub(crate) ledger: Ledger,
    /// Calls in flight, across seats.
    pub(crate) calls: Arc<Semaphore>,
}

impl Shared {
    /// The seat whose bearer is `presented`. Every seat is compared, so the
    /// time taken says nothing about which one matched.
    pub(crate) fn seat(&self, presented: &[u8]) -> Option<Arc<SeatState>> {
        let seats = self.seats.read().unwrap_or_else(PoisonError::into_inner);
        let mut found = None;
        for seat in seats.iter() {
            if seat.bearer.matches(presented) {
                found = Some(seat.clone());
            }
        }
        found
    }
}

/// The loopback MCP server.
pub struct HiveToolServer {
    shared: Arc<Shared>,
    addr: SocketAddr,
    stop: watch::Sender<bool>,
    task: JoinHandle<()>,
    purge: JoinHandle<()>,
}

impl HiveToolServer {
    /// Bind a random port of 127.0.0.1 and start serving.
    ///
    /// # Errors
    /// [`Error::Listen`] when the listener cannot be opened.
    pub async fn start(services: Services, options: ServerOptions) -> Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .map_err(Error::Listen)?;
        let addr = listener.local_addr().map_err(Error::Listen)?;
        let shared = Arc::new(Shared {
            services,
            port: addr.port(),
            seats: RwLock::default(),
            ledger: Ledger::new(options.max_records, options.max_record_bytes),
            calls: Arc::new(Semaphore::new(options.max_calls.max(1))),
            options,
        });
        let (stop, stopped) = watch::channel(false);
        let task = tokio::spawn(serve(listener, shared.clone(), stopped.clone()));
        let purge = tokio::spawn(purge_ended(shared.clone(), stopped));
        Ok(Self {
            shared,
            addr,
            stop,
            task,
            purge,
        })
    }

    /// Where the server listens.
    #[must_use]
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// The URL every seat is served at.
    #[must_use]
    pub fn url(&self) -> String {
        format!("http://{}/mcp", self.addr)
    }

    /// Serve `seat` behind a fresh bearer, and return its endpoint.
    ///
    /// # Errors
    /// [`Error::DuplicateSeat`], [`Error::DuplicateTool`],
    /// [`Error::ReservedArgument`] or [`Error::Random`].
    pub fn serve_seat(&self, seat: SeatTools) -> Result<ToolEndpoint> {
        let managed = self.shared.services.management.is_some();
        let per_seat = self.shared.options.max_calls_per_seat;
        let bearer = Bearer::generate()?;
        let state = SeatState::new(seat, bearer, managed, &self.shared.services, per_seat)?;
        let mut seats = self
            .shared
            .seats
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        let id = state.tools.seat().to_owned();
        if seats.iter().any(|served| served.tools.seat() == id) {
            return Err(Error::DuplicateSeat(id));
        }
        let endpoint = ToolEndpoint::new(
            self.shared.options.name.clone(),
            self.url(),
            state.bearer.expose(),
            state.catalog.names(),
        );
        seats.push(Arc::new(state));
        Ok(endpoint)
    }

    /// Stop serving the seat with this id; its bearer is refused from now
    /// on. Whether it was served.
    pub fn withdraw_seat(&self, seat_id: &str) -> bool {
        let mut seats = self
            .shared
            .seats
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        let before = seats.len();
        seats.retain(|served| served.tools.seat() != seat_id);
        drop(seats);
        self.shared.ledger.forget(seat_id);
        before
            != self
                .shared
                .seats
                .read()
                .unwrap_or_else(PoisonError::into_inner)
                .len()
    }

    /// Stop accepting, let open requests finish within the grace period, and
    /// close the listener.
    ///
    /// # Errors
    /// [`Error::Stopped`] when the server task panicked.
    pub async fn shutdown(mut self) -> Result<()> {
        let _ = self.stop.send(true);
        let purged = (&mut self.purge).await;
        let served = (&mut self.task).await;
        served
            .and(purged)
            .map_err(|error| Error::Stopped(error.to_string()))
    }
}

impl Drop for HiveToolServer {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

impl fmt::Debug for HiveToolServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let seats = self
            .shared
            .seats
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        let ids: Vec<&str> = seats.iter().map(|seat| seat.tools.seat()).collect();
        f.debug_struct("HiveToolServer")
            .field("addr", &self.addr)
            .field("seats", &ids)
            .finish_non_exhaustive()
    }
}

/// Accept connections until told to stop, then wind them down.
///
/// Accepting is not bounded: an idle or unauthenticated connection is closed
/// after [`ServerOptions::header_timeout`] and holds no call slot, so it
/// cannot keep a seat waiting. What is bounded is calls in flight.
async fn serve(listener: TcpListener, shared: Arc<Shared>, mut stopped: watch::Receiver<bool>) {
    let graceful = GracefulShutdown::new();
    loop {
        let stream = tokio::select! {
            _ = stopped.wait_for(|stop| *stop) => break,
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => stream,
                Err(_) => continue,
            },
        };
        let handler = shared.clone();
        let service = service_fn(move |request| http::handle(handler.clone(), request));
        let connection = http1::Builder::new()
            .timer(TokioTimer::new())
            .header_read_timeout(shared.options.header_timeout)
            .serve_connection(TokioIo::new(stream), service);
        let connection = graceful.watch(connection);
        tokio::spawn(connection);
    }
    drop(listener);
    let _ = tokio::time::timeout(shared.options.shutdown_grace, graceful.shutdown()).await;
}

/// Drop the call records of every turn that has ended, as the coordinator
/// commits.
async fn purge_ended(shared: Arc<Shared>, mut stopped: watch::Receiver<bool>) {
    let coordinator = shared.services.coordinator.clone();
    let mut revisions = coordinator.subscribe();
    loop {
        tokio::select! {
            _ = stopped.wait_for(|stop| *stop) => break,
            changed = revisions.changed() => {
                if changed.is_err() {
                    break;
                }
                shared
                    .ledger
                    .purge(|seat| coordinator.running_turn(seat).ok().flatten());
            }
        }
    }
}
