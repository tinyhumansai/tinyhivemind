//! Permanent hive tools for host-supplied `OpenHuman` agents.
//!
//! Build agents on one runtime, then hand their existing handles to
//! [`OpenHumanHost`]. Their configuration and continuing sessions stay owned by
//! `OpenHuman`; the coordinator owns attributed messaging and scheduling.
//! With [`OpenHumanHost::with_hive_memory`], every seat registered afterwards
//! shares one [`HiveMemory`]: its own memory agent id under the hive's root.
//! ```no_run
//! # async fn example(agent: openhuman_embed::Agent, coordinator: tinyhivemind_hives::Coordinator) -> tinyhivemind_openhuman::Result<()> {
//! let host = tinyhivemind_openhuman::OpenHumanHost::new(agent.runtime_id().into(), coordinator)?;
//! host.register_agent(agent).await?;
//! # Ok(()) }
//! ```
//!
//! # Deploying a hive package
//!
//! A hive is defined by one `tinyhivemind-lang` package: structure,
//! deliberation policy and everything its agents run with (provider, models,
//! MCP servers, skills, permissions, workflows, executable tools and hooks).
//! The earlier `HiveConfig` manifests are gone as an input; the deployment plan
//! the adapter builds from is crate-internal and is produced from packages.
//! [`language::PackageSource::load`] reads and checks a directory, and
//! [`deploy::HiveDeployment::from_packages`] deploys one or more of them onto a
//! single runtime, attaching each seat's executable tools and script hooks
//! (run by `tinyhivemind-exec`).
//!
//! ```no_run
//! use std::sync::Arc;
//! use tinyhivemind_openhuman::deploy::{BuildOptions, EnvironmentSecrets, HiveDeployment};
//! use tinyhivemind_openhuman::language::PackageSource;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let package = PackageSource::load("hives/release-desk")?;
//! let deployment = Box::pin(HiveDeployment::from_packages(
//!     vec![package],
//!     Arc::new(EnvironmentSecrets),
//!     BuildOptions::default(),
//! ))
//! .await?;
//! let _host = deployment.host();
//! # Ok(()) }
//! ```
//!
//! What this crate does not hold: the host owns storage, transcripts and
//! scheduling, and the language crate owns the definition, so the adapter keeps
//! no schema of its own that a package could drift from.
pub(crate) mod config;
pub mod deploy;
mod error;
mod host;
pub mod journal;
pub mod language;
mod memory;
#[cfg(any(test, feature = "offline"))]
pub mod offline;
mod tools;
pub use error::{ConfigError, Error, Result};
pub use host::{
    AgentFactory, AgentFuture, HostedTurn, ManagementAuthorizer, ManagementRequest, OpenHumanHost,
    RegisteredAgent, SendAuthorizer, SendRequest, TURN_TIMEOUT, TurnHooks, TurnOptions,
    TurnProgressSink, TurnScope,
};
pub use journal::MemoryLog;
pub use memory::{HiveMemory, HiveMemoryStore};
#[cfg(test)]
pub(crate) static RUNTIME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
