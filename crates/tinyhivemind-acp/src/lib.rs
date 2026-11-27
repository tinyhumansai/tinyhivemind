//! Hive seats on any Agent Client Protocol agent, each a supervised process.
//!
//! [`AcpHarness`] implements the host's
//! [`Harness`](tinyhivemind_host::harness::Harness) port once, for every
//! agent that speaks the [Agent Client Protocol](https://agentclientprotocol.com)
//! over stdio. What differs between agents is an [`Agent`]: the command, its
//! environment and files, the `session/new` metadata that turns its built-in
//! tools off, and the session mode. [`Agent::claude_code`],
//! [`Agent::codex`] and [`Agent::openclaw`] are the three measured in
//! `docs/experiments/2026-11-26-acp-harnesses.md`. `OpenClaw`'s bridge takes its
//! tools from a shared Gateway rather than per session; [`openclaw`] plans and
//! supervises that Gateway.
//!
//! A seat is one agent process in its own private directory. Its first turn
//! opens an ACP session with the host's tool server as its one MCP server
//! (or none, for an agent given it elsewhere, as `OpenClaw`'s Gateway is);
//! every turn is a `session/prompt` on that session, and a restarted process
//! resumes it with `session/load`. A permission request for one of the host's
//! tools is allowed here, since the tool server already judges hive tools;
//! any other goes to the seat's approver.
//!
//! ```no_run
//! use tinyhivemind_acp::{AcpHarness, Agent};
//! use tinyhivemind_host::harness::{Harness, SeatPlan, ToolEndpoint};
//!
//! # async fn start(seat: tinyhivemind_lang::LoweredSeat) -> Result<(), Box<dyn std::error::Error>> {
//! let agent = Agent::claude_code(
//!     "/usr/bin/node",
//!     "/opt/acp/claude/node_modules/@agentclientprotocol/claude-agent-acp/dist/index.js",
//!     "0.112.0",
//! );
//! let harness = AcpHarness::new(agent);
//! let tools = ToolEndpoint::new("hive", "http://127.0.0.1:7000/mcp", "seat-bearer", ["hivemind_post".to_owned()]);
//! let runner = harness.start(SeatPlan::new(seat, "/var/lib/hive/scout"), tools).await?;
//! // Register `runner` with the coordinator; stop the seat with:
//! harness.stop("scout").await?;
//! # drop(runner); Ok(()) }
//! ```
//!
//! What this crate does not hold: it links no agent and installs none (the
//! host passes the paths of pinned installs); it serves no tools (the host's
//! MCP server does); and it cannot observe the tool list an agent shows its
//! model, which ACP does not expose, so that is held by each descriptor's
//! settings and proven by the live tests rather than checked at start.
pub mod agent;
pub mod capability;
mod connection;
mod error;
mod harness;
pub mod openclaw;
mod protocol;
mod seat;

pub use agent::{Agent, CatalogError};
pub use harness::{AcpHarness, Options};
