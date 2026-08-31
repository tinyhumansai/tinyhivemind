//! Runtime-neutral session coordination for agent group chats.
//!
//! `tinyteams` re-exports the pure [`tinyteams_core`] algebra and adds the
//! boundaries that must wait on a host: a [`SessionLog`] paging port,
//! attributed transcript projection, ephemeral team initialization, and the
//! narrow [`Selector`] port used by model-assisted responder choice.
//! The host remains responsible for storage, transports, model clients, and
//! choosing an async executor.
//!
//! # Example
//!
//! ```
//! use tinyteams::{BriefedTeammate, TeamBriefing};
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
pub mod error;
pub mod responder;
pub mod session;
pub mod sharing;

pub use briefing::{BriefedTeammate, SessionInitialization, TeamBriefing, initialize_session};
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
pub use tinyteams_core::*;
