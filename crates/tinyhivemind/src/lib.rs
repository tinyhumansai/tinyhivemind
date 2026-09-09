//! Runtime-neutral session coordination for a hive of agents.
//!
//! `tinyhivemind` re-exports the pure [`tinyhivemind_core`] algebra and adds the
//! boundaries that must wait on a host: a [`SessionLog`] paging port,
//! attributed transcript projection, ephemeral team initialization, a bounded
//! index of a desk's live threads, [`search`] and [`pins`] over
//! that same log, and the
//! narrow [`Selector`] port used by model-assisted responder choice. A channel
//! that outgrows every window is folded by [`mod@digest`] into one bounded,
//! superseding account behind a live tail, through a second narrow port.
//! Mention dispatch remains pure until one canonical request reaches the
//! host-owned atomic [`MentionTurnQueue`] boundary, and [`mod@referral`] extends
//! that same edge across a channel: one child turn that may run on another
//! desk, and the one answer that comes back.
//! The host remains responsible for storage, transports, model clients, and
//! choosing an async executor.
//!
//! # Example
//!
//! ```
//! use tinyhivemind::{BriefedTeammate, TeamBriefing};
//!
//! let briefing = TeamBriefing {
//!     viewer_id: "alice".into(),
//!     desk_id: "engineering".into(),
//!     desk_name: "Engineering".into(),
//!     teammates: vec![BriefedTeammate {
//!         id: "bob".into(),
//!         label: "Bob".into(),
//!         role: Some("reviewer".into()),
//!         description: None,
//!     }],
//!     brevity: Default::default(),
//!     asides: Default::default(),
//! };
//! assert!(briefing.system_text().contains("@bob"));
//! ```
//!
//! # Seeing the two edges run
//!
//! The `crosstalk` example seats real models on one desk and prints what each
//! turn was shown: an unaddressed instruction routed by [`choose_responder`],
//! an agent's reply addressing a peer by name, and the one child turn
//! [`dispatch_mention`] allows out of it, bounded by the host's hop budget.
//!
//! ```sh
//! cargo run -p tinyhivemind --example crosstalk -- \
//!   --api-base http://127.0.0.1:6969 --model flash --thread
//! ```
//!
//! It needs a live endpoint or an agent CLI, so it is an example rather than a
//! test. `crates/tinyhivemind/examples/crosstalk/README.md` says what it does and
//! does not establish — in particular that addressing a peer is not a private
//! message, because nothing in this crate restricts who may read a row.

pub mod briefing;
pub mod digest;
pub mod dispatch;
pub mod error;
pub mod pins;
pub mod referral;
pub mod responder;
pub mod search;
pub mod session;
pub mod sharing;
pub mod speech;
pub mod threads;

pub use briefing::{
    BrevityPolicy, BriefedTeammate, BriefingNote, MentionDispatchContext, SessionContext,
    SessionInitialization, TeamBriefing, initialize_session, initialize_session_with_context,
};
pub use digest::{
    ChannelDigest, ChannelHead, DigestFuture, DigestOutcome, DigestPlan, DigestPolicy, DigestRejection,
    DigestRequest, DigestedHistory, Digester, accept_digest, apply_digest, collect_digest_input,
    plan_digest, refold,
};
pub use dispatch::{
    EnqueueOutcome, EnqueueRefusal, MentionDispatchOutcome, MentionTurnFuture, MentionTurnQueue,
    dispatch_mention,
};
pub use error::{Error, Result};
pub use pins::{
    PIN_EXCERPT_CHARS, PIN_LIMIT, PIN_SCAN, Pin, PinAction, PinDirective, fold_pins, pin_note,
    read_directives, read_pinboard,
};
pub use referral::{
    NoReferralReason, Referral, ReferralDecision, ReferralFuture, ReferralInput, ReferralKind,
    ReferralOrigin, ReferralOutcome, ReferralPolicy, ReferralQueue, ReferralReach,
    dispatch_referral, referral,
};
pub use responder::{BoxError, Selector, SelectorFuture, choose_responder};
pub use search::{
    EXCERPT_CHARS, MessageHit, SEARCH_LIMIT, SEARCH_SCAN, SearchPattern, SearchQuery, ThreadHit,
    search_messages, search_threads,
};
pub use session::{
    Conversation, Elision, LogMessage, PAGE_SIZE, SCAN_LIMIT, SESSION_WINDOW, Sequence,
    SessionAuthor, SessionFuture, SessionLog, SessionMessage, SessionPage, SessionQuery,
    SourceError, project_as, project_session,
};
pub use sharing::{
    PRESENT_SET_LIMIT, ReinitializeReason, SessionDelta, SharingPlan, SharingQuery, SharingState,
    initialized_state, note_present, prepare_delta,
};
pub use speech::{
    CallArguments, CommitRequest, CommittedUtterance, ToolCall, ToolParameter, ToolSpec, Utterance,
    UtteranceRejection, addressed_peers, check_recipients, commit_utterance, interpret, tool_specs,
};
pub use threads::{
    THREAD_INDEX_LIMIT, THREAD_INDEX_SCAN, THREAD_OPENING_CHARS, ThreadLine, fold_thread_index,
    read_thread_index,
};
pub use tinyhivemind_core::*;
