//! Typed failures from malformed hive inputs.

#[cfg(test)]
mod test;

use thiserror::Error;

/// A failure produced while folding a deliberation episode.
///
/// Every variant names a specific malformed input. Nothing here reports an IO
/// failure, because this crate performs none.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// A roster or desk snapshot was structurally invalid.
    #[error("{source}")]
    Core {
        /// The underlying algebra failure.
        #[source]
        source: tinyteams::error::Error,
    },
    /// A topic identity was empty.
    #[error("topic id must not be empty")]
    EmptyTopicId,
    /// Two threshold records named the same agent.
    #[error("duplicate agent threshold `{agent_id}`")]
    DuplicateAgentThreshold {
        /// The repeated agent id.
        agent_id: String,
    },
    /// A threshold record named an agent that is not an active desk member.
    #[error("threshold `{agent_id}` is not an active member of desk `{desk_id}`")]
    UnknownThresholdMember {
        /// The offending agent id.
        agent_id: String,
        /// The desk the episode runs on.
        desk_id: String,
    },
    /// A salience half-life of zero would make recency undefined.
    #[error("salience half life must not be zero")]
    ZeroHalfLife,
    /// A quorum threshold of zero would carry every topic immediately.
    #[error("quorum threshold must not be zero")]
    ZeroQuorumThreshold,
    /// A quorum window of zero would admit no support at all.
    #[error("quorum window must not be zero")]
    ZeroQuorumWindow,
    /// The spent-turn counter could not be advanced.
    #[error("episode spend overflowed at {spent}")]
    SpendOverflow {
        /// The counter value that could not be advanced.
        spent: u32,
    },
}

impl From<tinyteams::error::Error> for Error {
    fn from(source: tinyteams::error::Error) -> Self {
        Self::Core { source }
    }
}

impl From<tinyteams::tinyteams_core_error::Error> for Error {
    fn from(source: tinyteams::tinyteams_core_error::Error) -> Self {
        Self::Core {
            source: source.into(),
        }
    }
}

/// The crate-wide result alias.
pub type Result<T> = std::result::Result<T, Error>;
