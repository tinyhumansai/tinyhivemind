//! The internal deployment plan: what `deploy` builds one runtime from.
//!
//! Hive packages (`tinyhivemind-lang`) are the only input; `language::plan`
//! lowers them into these types, and `validate` checks the cross-package
//! authority rules a single package cannot.
mod parse;
mod types;
mod validate;
pub(crate) use crate::error::ConfigError;
pub(crate) use types::*;
/// Manifest parsing and validation result.
pub(crate) type Result<T> = std::result::Result<T, ConfigError>;
#[cfg(test)]
mod test;
