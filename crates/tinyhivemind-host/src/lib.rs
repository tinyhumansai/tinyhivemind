//! Harness-neutral hosting for hive seats.
//!
//! A host runs a hive's seats on some agent harness. This crate is the part of
//! that which is the same whichever harness it is:
//!
//! - [`tools`]: the hive tools, and the [`tools::Belt`] that executes a call as
//!   the seat the host bound it to.
//! - [`policy`]: the ports through which a host gates sends and management.
//! - [`turn`]: a turn's scope, options and prompt.
//! - [`language`] and [`config`]: hive packages, read, checked and lowered into
//!   the deployment plan an adapter builds from.
//! - [`activation`]: the barrier between attaching tools and registration.
//! - [`journal`]: an optional in-memory session log.
//! - [`process`] and [`harness`]: harnesses run as supervised child processes.
//!
//! ```
//! use serde_json::json;
//! use std::sync::Arc;
//! use tinyhivemind_hives::{Coordinator, CoordinatorOptions, MemoryStorage};
//! use tinyhivemind_host::tools::{Belt, Kind, ToolCall};
//!
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let coordinator = Coordinator::new(
//!     "runtime".into(),
//!     Arc::new(MemoryStorage::new()),
//!     CoordinatorOptions::default(),
//! )
//! .await?;
//! let belt = Belt::new(&coordinator);
//!
//! // The host supplies the caller; an argument that names one is refused.
//! let call = ToolCall::new("scout", None, Kind::ListHives, json!({}))?;
//! assert_eq!(belt.call(&call).await?, json!([]));
//! assert!(ToolCall::new("scout", None, Kind::ListHives, json!({"sender": "lead"})).is_err());
//! # Ok(()) }
//! ```
//!
//! What this crate does not hold: it names no harness type and links none (ADR
//! 0035), so an adapter supplies the harness's answers ([`config::HarnessRules`])
//! and builds its agents ([`policy::AgentCreator`]). It holds no transcript and
//! no durable state: the coordinator in `tinyhivemind-hives` owns both.

pub mod activation;
pub mod config;
mod error;
pub mod harness;
pub mod journal;
pub mod language;
pub mod policy;
pub mod process;
pub mod tools;
pub mod turn;
pub use error::{ArgumentError, Error, Result};
