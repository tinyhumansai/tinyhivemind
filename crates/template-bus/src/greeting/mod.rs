//! The payloads the `Greet` member exchanges.
//!
//! A module root like this one documents the module, wires its pieces together,
//! and exposes the smallest useful API. The type definitions live in the
//! sibling `types.rs`, and the unit tests in `test.rs`, wired in at the bottom
//! of this file.
//!
//! Replace this module with the first real payload family the module carries.
//! Payload types are `serde`-derived, `#[non_exhaustive]`, and hold owned data:
//! they are decoded from a frame, so they can borrow nothing from the caller.

mod types;

pub use types::{GreetRequest, GreetResponse};

#[cfg(test)]
mod test;
