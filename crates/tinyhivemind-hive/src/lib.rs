//! Bounded group deliberation for agent group chats.
//!
//! `tinyhivemind-hive` adds the one thing a shared desk cannot do on its own:
//! **converge**. A message today selects exactly one responder off a
//! deterministic ladder, that agent replies, and the interaction ends. This
//! crate lets a room of agents put proposals side by side, accumulate support,
//! register a grounded objection, and terminate for a reason it can name.
//!
//! # An episode is a sequence of single turns
//!
//! A hive mind is normally built as fan-out — publish a task, wake N agents,
//! gather the replies. This crate deliberately does not. [`HiveStep::Speak`]
//! carries exactly one [`HiveTurn`], so the charter's *one message, one turn*
//! rule is a type invariant rather than a convention.
//!
//! That is not only a safety constraint. Sparse communication topologies match
//! or beat fully connected ones in multi-agent debate at much lower cost;
//! conformity rises with interaction time, so convergence is a warning signal
//! as much as a success signal; and parallel fan-out wins only on genuinely
//! decomposable work. The one thing fan-out really buys is *independence*, and
//! this crate buys that as [`Visibility`] — a filter on what one turn sees —
//! rather than as concurrency. See
//! `docs/adr/0002-hive-episodes-are-sequential.md`.
//!
//! # What this crate deliberately does not hold
//!
//! - **A port.** There is no trait here for a host to implement. An episode is
//!   [`step`], a fold over a transcript the caller already holds, and the host
//!   does its waiting through the [`SessionLog`], [`Selector`] and
//!   [`MentionTurnQueue`] ports `tinyhivemind` already defines.
//! - **Storage.** [`EpisodeState`] is returned, never applied. The caller
//!   commits it after its turn is durably appended.
//! - **Floating point.** Every score is fixed-point integer, so every payload
//!   derives [`Eq`] and every fold is reproducible.
//! - **A quality claim.** This is a protocol for bounded deliberation. Nothing
//!   here is shown to make answers better, and almost every positive
//!   multi-agent result in the literature is confounded by compute.
//!
//! [`SessionLog`]: tinyhivemind::SessionLog
//! [`Selector`]: tinyhivemind::Selector
//! [`MentionTurnQueue`]: tinyhivemind::MentionTurnQueue
//!
//! # Modules
//!
//! - [`attention`] — the bid each member makes for the floor, and the argmax.
//! - [`episode`] — the pure state machine, and the visibility filter.
//! - [`error`] — typed failures from malformed inputs.
//! - [`quorum`] — standings, cross-inhibition, and the consensus predicate.
//! - [`mod@salience`] — recency decay, importance, and relevance.
//! - [`trace`] — the stigmergic grammar and its read.
//!
//! # Example
//!
//! ```
//! use tinyhivemind::{SessionAuthor, SessionMessage, Sequence};
//! use tinyhivemind_hive::{
//!     quorum::{consensus, standings, ConsensusState, QuorumPolicy},
//!     trace::read,
//! };
//!
//! fn agent(id: &str) -> SessionAuthor {
//!     SessionAuthor::Agent { id: id.into(), label: id.into() }
//! }
//! fn said(sequence: u64, author: SessionAuthor, content: &str) -> SessionMessage {
//!     SessionMessage { sequence: Sequence(sequence), author, content: content.into() }
//! }
//!
//! // Two agents propose; a third grounds its support in the first proposal.
//! let transcript = [
//!     said(1, agent("planner"), "!propose #stage Stage the rollout."),
//!     said(2, agent("scout"), "!propose #ship Ship it all at once."),
//!     said(3, agent("critic"), "!support #stage ^1 Staging bounds the blast radius."),
//!     said(4, agent("planner"), "!support #stage ^1 Agreed, and it is reversible."),
//! ];
//!
//! let traces = read(&transcript);
//! let policy = QuorumPolicy { threshold: 2, window: 100, require_grounded: true };
//! let standings = standings(&traces, Sequence(4), &policy)?;
//!
//! // Two distinct grounded supporters carry `stage`; `ship` has none.
//! assert_eq!(consensus(&standings, &policy), ConsensusState::Quorum { topic: "stage".into() });
//! # Ok::<(), tinyhivemind_hive::error::Error>(())
//! ```

pub mod attention;
pub mod episode;
pub mod error;
pub mod quorum;
pub mod salience;
pub mod trace;

pub use attention::{AgentThreshold, Bid, BidReason, bids, floor_holder};
pub use episode::{
    EpisodePolicy, EpisodeState, HiveStep, HiveTurn, Phase, Visibility, project_for, step,
};
pub use error::{Error, Result};
pub use quorum::{ConsensusState, QuorumPolicy, TopicStanding, consensus, standings};
pub use salience::{Salience, SalienceWeights, salience};
pub use trace::{TRACE_CAP, TopicId, Trace, TraceKind, read, resolve};
// A host that wants group deliberation takes this crate alone and gets the
// session runtime and the pure algebra with it, so the types it hands to
// `step` are the *same* types rather than structural twins. This crate's own
// `error`, `Error` and `Result` deliberately shadow the runtime's.
pub use tinyhivemind::*;
