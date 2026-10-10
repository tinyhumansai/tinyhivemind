//! Declarative hive packages, validated and lowered without opening storage.
//!
//! Hosts supply strings to [`parse`], retain the immutable constitution, and
//! activate [`lower`] results between turns. This crate runs no agents.
//!
//! ```
//! use tinyhivemind_lang::{validate, Package};
//! let package = Package::default();
//! assert!(validate(&package).is_ok());
//! ```
pub mod document;
pub mod error;
pub mod lineage;
pub mod lowering;
pub mod memory;
pub mod patch;
pub mod validation;
pub use document::*;
pub use error::{Error, Result};
pub use lowering::{LoweredHive, LoweredMemoryBinding, LoweredSeat, lower};
pub use memory::*;
pub use validation::validate;
