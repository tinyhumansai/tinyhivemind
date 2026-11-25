//! Hive seats on OpenHuman, each a supervised `openhuman-core` process.
//!
//! [`OpenHumanHarness`] implements the host's
//! [`Harness`](tinyhivemind_host::harness::Harness) port for the `openhuman`
//! kind. Each seat gets one `openhuman-core run --headless-api` in its own
//! private directory, with a generated `config.toml`, and is driven over its
//! loopback JSON-RPC and `/events` stream: a turn is `channel_web_chat` on
//! the seat's one thread, followed by its events until `chat_done` or
//! `chat_error`, and cancelled with `channel_web_cancel`.
//!
//! Before a seat starts, [`capability::check`] holds it to the `openhuman`
//! column of the capability table in `docs/specs/harnesses.md`: only an
//! `openai` provider with an explicit endpoint and no credential, no more
//! than [`capability::ITERATION_CAP`] iterations, no sandbox, no built-in
//! tools. A start is verified: the process answering is the child, its
//! version is the pin, its effective config is the one written, and its
//! model sees exactly the host's tools.
//!
//! ```no_run
//! use std::sync::Arc;
//! use tinyhivemind_host::harness::{Harness, SeatPlan, ToolEndpoint};
//! use tinyhivemind_openhuman_process::{OpenHumanHarness, Program};
//!
//! # async fn start(seat: tinyhivemind_lang::LoweredSeat) -> Result<(), Box<dyn std::error::Error>> {
//! let harness = OpenHumanHarness::new(Program::new("/opt/oh/openhuman-core", "0.64.15"));
//! let tools = ToolEndpoint::new(
//!     "hivemind",
//!     "http://127.0.0.1:7000/mcp",
//!     "seat-bearer",
//!     ["hivemind_post".to_owned()],
//! );
//! let runner = harness.start(SeatPlan::new(seat, "/var/lib/hive/scout"), tools).await?;
//! // Register `runner` with the coordinator; stop the seat with:
//! harness.stop("scout").await?;
//! # drop(runner); Ok(()) }
//! ```
//!
//! What this crate does not hold: it names no OpenHuman type and links no
//! harness, so it outlives the linked adapter; it serves no tools (the host's
//! MCP server does, and judges their policy); and it never downloads a
//! binary: `.github/scripts/fetch-harness.sh` fetches the one pinned in
//! `harnesses.lock`, and the host passes its path.
pub mod capability;
mod config;
mod error;
mod harness;
mod instance;
mod rpc;
mod turn;

pub use harness::{OpenHumanHarness, Options, Program};

#[cfg(test)]
#[path = "../tests/support/http.rs"]
mod http;
#[cfg(test)]
#[path = "../tests/fake/surface.rs"]
mod fake;
#[cfg(test)]
mod fixture;
