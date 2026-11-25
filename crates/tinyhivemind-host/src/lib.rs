//! Harness-neutral hosting for hive seats.
//!
//! This crate names no harness type and links none. It holds no transcript and
//! no durable state: the coordinator in `tinyhivemind-hives` owns both.

pub mod harness;
pub mod process;
