//! A hive's seats on their harness processes, started together under one
//! coordinator.
//!
//! [`ProcessSeats::start`] is the host's half of a mixed deployment
//! (`docs/plans/2026-11-25-process-harnesses.md`, step 5): given a
//! coordinator, the seats a package lowers and where each harness kind's
//! pinned install is ([`Installs`]), it
//!
//! 1. checks every seat can run here, before anything starts;
//! 2. serves every seat's hive tools and executable tools from one
//!    `tinyhivemind-mcp` server, one bearer per seat;
//! 3. starts the `OpenClaw` Gateways the `OpenClaw` seats need, each
//!    configured with its seats' tool servers: one shared by every seat of
//!    `deployment` isolation, the kind's default, and one per seat of `seat`
//!    isolation;
//! 4. starts each seat on the harness its kind maps to (`OpenHuman`'s process
//!    client, or the ACP client for Claude Code, Codex and `OpenClaw`) and
//!    registers it with the coordinator.
//!
//! A failure partway stops whatever had started. [`ProcessSeats::stop`] stops
//! the seats, then the Gateways, then the tool server.
//!
//! ```no_run
//! use tinyhivemind_deploy::{Deployment, Installs, Pinned, ProcessSeat, ProcessSeats};
//! use tinyhivemind_mcp::Services;
//!
//! # async fn run(
//! #     coordinator: tinyhivemind_hives::Coordinator,
//! #     lead: tinyhivemind_lang::LoweredSeat,
//! # ) -> Result<(), Box<dyn std::error::Error>> {
//! let installs = Installs {
//!     node: Some("/opt/node/bin/node".into()),
//!     claude_code: Some(Pinned::new(
//!         "/opt/acp/claude/node_modules/@agentclientprotocol/claude-agent-acp/dist/index.js",
//!         "0.112.0",
//!     )),
//!     ..Installs::default()
//! };
//! let seats = vec![ProcessSeat::new(lead, "/var/lib/hive/seats/lead")];
//! let services = Services::new(coordinator.clone());
//! let deployment = Deployment::new("/var/lib/hive/deployment");
//! let mut running =
//!     ProcessSeats::start(&coordinator, services, seats, &installs, &deployment).await?;
//! // Create the hives, send work, run the coordinator; then:
//! running.stop().await?;
//! # Ok(()) }
//! ```
//!
//! What this crate does not hold: it links no harness and installs none (the
//! host passes the paths of pinned installs); it creates no hive and sends no
//! message (the host does, on the coordinator it passes in); and it does not
//! choose a seat's harness, which the package declares.
mod error;
#[cfg(test)]
mod fixture;
mod gateway;
mod installs;
mod seats;

pub use error::{Error, Result};
pub use installs::{CodexInstall, Installs, Pinned};
pub use seats::{Deployment, ProcessSeat, ProcessSeats};
