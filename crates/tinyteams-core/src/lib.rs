//! Desks, rosters, mentions and shared session transcripts for agent group
//! chats — the pure algebra behind a room that several agents share.
//!
//! This crate answers four questions, and holds no state while doing it:
//!
//! - **Who is here?** A roster of teammates and the people signed in with them.
//! - **What is a desk, and who is on it?** A blueprint-declared group chat
//!   merged with the operator's runtime additions, retirements and ordering.
//! - **Who does `@this` mean?** The mention grammar, and the resolution of a
//!   name against the roster and the desks.
//! - **What does one participant see of the shared transcript?** The projection
//!   of a multi-speaker session into one viewer's turn history.
//!
//! # No IO, and why it matters
//!
//! Everything here is a fold over data the caller already holds. There is no
//! async, no storage, no journal, no transport, and nothing that returns a
//! `Result` for an IO reason. The caller does the one roster read and the one
//! transcript read and hands the results in.
//!
//! That is not an aesthetic preference. This crate sits on the hot path of
//! every agent turn — addressing a message, resolving a mention, projecting a
//! transcript — so it has to be cheap, and it has to compile in a host's
//! default build with no feature flags behind it. It is also what keeps the
//! dependency arrow pointing one way: a crate that cannot call out cannot grow
//! a path back into its host. `.github/scripts/assert-pure.sh` asserts it.
//!
//! The stateful half — the paging walk over a session log, the responder
//! ladder, and the mention-dispatch edge — lives in the sibling `tinyteams`
//! crate, which owns the ports a host implements.
//!
//! # Layout
//!
//! - `src/error/` holds the crate-wide [`Error`] enum and the [`Result`] alias
//!   returned by every fallible public function.
//! - Each feature area lives in its own module directory with a `mod.rs`
//!   module root, an optional `types.rs`, and a `test.rs` holding its unit
//!   tests.
//! - Every public item is re-exported from here, so downstream users have a
//!   single predictable surface.
//!
//! # Status
//!
//! The `greeting` module is the tinyteams_core's placeholder. It stays until the
//! first real feature area lands so the coverage gate has something to measure,
//! and is deleted then.

mod error;
mod greeting;

pub use error::{Error, Result};
pub use greeting::greet;
