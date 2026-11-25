//! Hive tools for harness processes, over MCP on loopback.
//!
//! A harness that runs as a child process cannot be handed a native tool, so
//! [`HiveToolServer`] serves a seat's hive tools and its package's
//! executable tools over the Streamable HTTP transport of the Model Context
//! Protocol, on a random port of 127.0.0.1 (ADR 0036):
//!
//! - **Identity.** [`HiveToolServer::serve_seat`] draws a random bearer per
//!   seat and returns its [`ToolEndpoint`]. The bearer alone decides the
//!   actor; it is compared in constant time and never printed.
//! - **Turn and call.** Every state-changing tool's schema gets two required
//!   arguments, `turn` and `call`, removed again before hooks and dispatch.
//!   A call whose `turn` is not the seat's running turn is refused. The
//!   identity (seat, turn, call) makes a retry return the first result: a
//!   coordinator action is recorded by the coordinator in the transaction of
//!   its effect, and an executable tool, `create_agent` or any call a hook
//!   matches is recorded here, started and finished, in memory.
//! - **Policy.** Package hooks, permission tool names and the `autonomy` and
//!   `access` gate run here, before dispatch; a supervised effectful tool is
//!   asked of the host's [`Approver`], and refused when there is none. Hive
//!   tools go through the host's send and management policy in the belt.
//! - **Surface.** Tools only; resource and prompt lists are empty. A bad
//!   `Host`, any `Origin`, an oversized body or a wrong bearer is refused,
//!   and no response is a redirect.
//!
//! ```
//! use std::sync::Arc;
//! use tinyhivemind_hives::{Coordinator, CoordinatorOptions, MemoryStorage};
//! use tinyhivemind_mcp::{HiveToolServer, SeatTools, ServerOptions, Services};
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let coordinator = Coordinator::new(
//!     "runtime".into(),
//!     Arc::new(MemoryStorage::new()),
//!     CoordinatorOptions::default(),
//! )
//! .await?;
//! let server = HiveToolServer::start(Services::new(coordinator), ServerOptions::default()).await?;
//! let endpoint = server.serve_seat(SeatTools::new("scout"))?;
//! assert!(endpoint.url.starts_with("http://127.0.0.1:"));
//! assert!(endpoint.tools.contains("hivemind_post"));
//! // The bearer goes into the harness's private config, never a log.
//! assert!(!format!("{endpoint:?}").contains(endpoint.bearer()));
//! server.shutdown().await?;
//! # Ok(()) }
//! ```
//!
//! What this crate does not hold: episode state, which is the coordinator's;
//! a harness, which a client crate starts and points at the endpoint; and a
//! proxy for a package's own MCP servers, so a seat declaring one is refused
//! by [`SeatTools::from_lowered`]. Nothing pure depends on it.
//!
//! [`ToolEndpoint`]: tinyhivemind_host::harness::ToolEndpoint
//! [`Approver`]: tinyhivemind_host::harness::Approver

mod error;
mod seat;
mod server;
mod tools;

pub use error::{Error, Result};
pub use seat::SeatTools;
pub use server::{HiveToolServer, PROTOCOL_VERSIONS, ServerOptions, Services};
