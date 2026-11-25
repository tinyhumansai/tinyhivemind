//! The cases every harness client must pass, over the port alone.
//!
//! A client crate enables the `conformance` feature as a dev-dependency,
//! implements [`Subject`] around its harness and a scripted model, and runs
//! every [`Case`], each on a fresh subject:
//!
//! ```text
//! #[tokio::test]
//! async fn conforms() -> Result<(), Failure> {
//!     for case in Case::ALL {
//!         case.run(&MySubject::new()).await?;
//!     }
//!     Ok(())
//! }
//! ```
//!
//! It is a feature rather than a test module because other crates call it, and
//! rather than always public because a deployed host has no use for it. Cases
//! return a [`Failure`] instead of panicking, so a client can also show that a
//! deliberately broken harness is caught.
//!
//! The cases that need hive tools over MCP (a call with the right actor, a
//! wrong bearer, another seat's bearer, a stale `turn`) are not here. They
//! belong with the server, in step 2 of the plan, and join [`Case`] then.
mod cases;
mod subject;

pub use subject::{CallState, EscapeFuture, ModelCall, ModelStep, Subject, ToolFault};

use std::fmt;

/// One case of the suite.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Case {
    /// A second turn runs in the first one's session, with its history.
    SecondTurnContinues,
    /// So does one after the seat was stopped and started by a new harness.
    ResumesAfterRestart,
    /// A session that is gone is an error, and no new one is started.
    ResumeFailureIsAnError,
    /// A turn past the seat's limit stops, and the seat goes on.
    TimeoutStopsTheTurn,
    /// Dropping a run stops its turn, and the seat goes on.
    DropStopsTheTurn,
    /// Killing the instance in a turn fails that turn for good, and the next
    /// one continues the session.
    KillNeitherReplaysNorLosesTheSeat,
    /// A second turn reaches the model only after the first is settled.
    OneTurnAtATime,
    /// A turn id that ran is refused.
    NoTurnTwice,
    /// Usage is each turn's own, also after a restart.
    UsageIsPerTurn,
    /// A seat starts only if the model sees exactly the expected tools, and a
    /// refused start leaves nothing running.
    ToolListIsExact,
    /// No process is left after a stop, not even one in its own session.
    NothingLeftAfterStop,
}

impl Case {
    /// Every case.
    pub const ALL: [Self; 11] = [
        Self::SecondTurnContinues,
        Self::ResumesAfterRestart,
        Self::ResumeFailureIsAnError,
        Self::TimeoutStopsTheTurn,
        Self::DropStopsTheTurn,
        Self::KillNeitherReplaysNorLosesTheSeat,
        Self::OneTurnAtATime,
        Self::NoTurnTwice,
        Self::UsageIsPerTurn,
        Self::ToolListIsExact,
        Self::NothingLeftAfterStop,
    ];

    /// Run this case on a fresh `subject`, within a Tokio runtime.
    ///
    /// # Errors
    /// What the harness did wrong.
    pub async fn run(self, subject: &dyn Subject) -> Result<(), Failure> {
        let ended = match self {
            Self::SecondTurnContinues => cases::second_turn_continues(subject).await,
            Self::ResumesAfterRestart => cases::resumes_after_restart(subject).await,
            Self::ResumeFailureIsAnError => cases::resume_failure_is_an_error(subject).await,
            Self::TimeoutStopsTheTurn => cases::timeout_stops_the_turn(subject).await,
            Self::DropStopsTheTurn => cases::drop_stops_the_turn(subject).await,
            Self::KillNeitherReplaysNorLosesTheSeat => cases::kill_mid_turn(subject).await,
            Self::OneTurnAtATime => cases::one_turn_at_a_time(subject).await,
            Self::NoTurnTwice => cases::no_turn_twice(subject).await,
            Self::UsageIsPerTurn => cases::usage_is_per_turn(subject).await,
            Self::ToolListIsExact => cases::tool_list_is_exact(subject).await,
            Self::NothingLeftAfterStop => cases::nothing_left_after_stop(subject).await,
        };
        ended.map_err(|reason| Failure { case: self, reason })
    }
}

/// A case a harness failed, and how.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Failure {
    /// The case.
    pub case: Case,
    /// What went wrong.
    pub reason: String,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.case, self.reason)
    }
}

impl std::error::Error for Failure {}
