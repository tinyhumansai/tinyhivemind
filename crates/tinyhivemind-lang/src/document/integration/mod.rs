//! Integration sections of `hive.json`: what a hive's agents run with.
//!
//! [`types`] holds the runtime, provider, model, MCP, skill, toolpack,
//! permission, profile, hook, session, Jev and workflow wire types; [`tool`]
//! holds the executable tool declaration. Validation lives in
//! `validation/integration.rs` and resolution in `lowering/`.
mod tool;
mod types;
pub use tool::*;
pub use types::*;
