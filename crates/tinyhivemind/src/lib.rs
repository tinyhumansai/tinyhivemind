//! Runtime-neutral session coordination for a hive of agents.
//!
//! `tinyhivemind` re-exports the pure [`tinyhivemind_core`] algebra and adds the
//! boundaries that must wait on a host: a [`SessionLog`] paging port,
//! attributed transcript projection, ephemeral team initialization, a bounded
//! index of a desk's live threads, and the
//! narrow [`Selector`] port used by model-assisted responder choice.
//! Mention dispatch remains pure until one canonical request reaches the
//! host-owned atomic [`MentionTurnQueue`] boundary.
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
//! };
//! assert!(briefing.system_text().contains("@bob"));
//! ```

pub mod briefing;
pub mod dispatch;
pub mod error;
pub mod responder;
pub mod session;
pub mod sharing;
pub mod threads;

pub use briefing::{
    BriefedTeammate, BriefingNote, SessionContext, SessionInitialization, TeamBriefing,
    initialize_session, initialize_session_with_context,
};
pub use dispatch::{
    EnqueueOutcome, EnqueueRefusal, MentionDispatchOutcome, MentionTurnFuture, MentionTurnQueue,
    dispatch_mention,
};
pub use error::{Error, Result};
pub use responder::{BoxError, Selector, SelectorFuture, choose_responder};
pub use session::{
    Conversation, LogMessage, PAGE_SIZE, SCAN_LIMIT, SESSION_WINDOW, Sequence, SessionAuthor,
    SessionFuture, SessionLog, SessionMessage, SessionPage, SessionQuery, SourceError,
    project_session,
};
pub use sharing::{
    PRESENT_SET_LIMIT, ReinitializeReason, SessionDelta, SharingPlan, SharingQuery, SharingState,
    initialized_state, note_present, prepare_delta,
};
pub use threads::{
    THREAD_INDEX_LIMIT, THREAD_INDEX_SCAN, THREAD_OPENING_CHARS, ThreadLine, fold_thread_index,
    read_thread_index,
};
pub use tinyhivemind_core::*;
