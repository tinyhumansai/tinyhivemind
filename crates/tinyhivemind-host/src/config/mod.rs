//! The deployment plan a harness adapter builds a hive's seats from.
//!
//! Hive packages (`tinyhivemind-lang`) are the only input;
//! [`crate::language::plan`] lowers them into these types, and
//! [`HiveConfig::validate_for`] checks the cross-package authority rules a
//! single package cannot. It opens no file and resolves no credential.
mod error;
mod parse;
mod rules;
mod types;
mod validate;
pub use error::ConfigError;
pub use rules::HarnessRules;
pub use types::*;
/// Plan validation result.
pub type Result<T> = std::result::Result<T, ConfigError>;
#[cfg(test)]
pub(crate) mod test;
