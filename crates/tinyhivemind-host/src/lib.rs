//! Harness-neutral hosting for hive seats.
//!
//! This crate names no harness type and links none. It holds no transcript and
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
