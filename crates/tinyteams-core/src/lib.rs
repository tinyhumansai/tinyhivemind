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
//! The waiting half — the paging walk over a session log, the optional model
//! selector call, and the mention-dispatch edge — lives in the sibling
//! `tinyteams` crate, which owns the ports a host implements. The responder
//! ladder's decisions remain pure here.
//!
//! # Layout
//!
//! Each feature area lives in its own module directory with a `mod.rs` module
//! root, an optional `types.rs`, and a `test.rs` holding its unit tests. The
//! public surface is namespaced by module rather than flattened here: this
//! crate grows to hold desks, rosters, mentions and session projection, and a
//! flat root would stop reading as four separable concerns.
//!
//! # Modules
//!
//! - [`chat`] — conversation identity: which stored chat id names which
//!   conversation, and the four spellings that mean the default desk.
//! - [`desk`] — host-compatible desk records and the borrowed overlay fold.
//! - [`error`] — typed failures from malformed records or unresolved desks.
//! - [`mention`] — authored mention parsing and pure routing choices.
//! - [`roster`] — borrowed agent and person identity snapshots.
//! - [`responder`] — deterministic selection of one agent for one message.
//!
//! # Example
//!
//! ```
//! use tinyteams_core::chat::{is_general_chat, same_conversation};
//!
//! // All four stored spellings of the default desk are one conversation.
//! assert!(is_general_chat(None));
//! assert!(same_conversation(Some("main"), Some("General")));
//!
//! // Everything else compares verbatim.
//! assert!(!same_conversation(Some("engineering"), Some("Engineering")));
//!
//! use tinyteams_core::desk::{Desk, DeskSet, ResponderMode};
//!
//! let desks = [Desk {
//!     id: "engineering".into(),
//!     name: "Engineering".into(),
//!     description: Some("Build the product".into()),
//!     members: vec!["alice".into(), "bob".into()],
//!     responder_mode: ResponderMode::Lead,
//! }];
//! let set = DeskSet::new(&desks, &[], &[], &[], &[]);
//! assert_eq!(set.lead("Engineering")?, Some("alice"));
//!
//! use tinyteams_core::{
//!     mention::{MentionAuthor, MentionTarget, direct_responder, resolve},
//!     roster::{Roster, RosterMember},
//! };
//! let members = [RosterMember {
//!     id: "alice".into(),
//!     name: Some("Alice".into()),
//! }];
//! let roster = Roster::new(&members, &[], &[]);
//! let mentions = resolve(
//!     "Could you take this, @Alice?",
//!     None,
//!     &MentionAuthor::Other,
//!     &roster,
//!     &set,
//! );
//! assert_eq!(direct_responder(&mentions, &roster), Some("alice"));
//! assert_eq!(mentions[0].target, MentionTarget::Agent { id: "alice".into() });
//! # Ok::<(), tinyteams_core::error::Error>(())
//! ```

pub mod chat;
pub mod desk;
pub mod error;
pub mod mention;
pub mod responder;
pub mod roster;
