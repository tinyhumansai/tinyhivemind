//! Typed runtime failures.

use crate::{Sequence, SourceError};

/// A session projection or initialization failure.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The host-owned session log could not be read.
    #[error("session log read failed")]
    Read {
        /// The host's original error.
        #[source]
        source: SourceError,
    },
    /// A page returned a row outside its exclusive upper bound.
    #[error("page row {sequence} is not before exclusive cursor {before}")]
    PageOutOfRange {
        /// The invalid row sequence.
        sequence: Sequence,
        /// The exclusive cursor used for the read.
        before: Sequence,
    },
    /// A page was not strictly newest-first.
    #[error("page rows are not strictly descending at {previous} then {next}")]
    PageNotDescending {
        /// The preceding sequence.
        previous: Sequence,
        /// The following sequence.
        next: Sequence,
    },
    /// A row sequence appeared more than once during one walk.
    #[error("duplicate session sequence {sequence}")]
    DuplicateSequence {
        /// The repeated sequence.
        sequence: Sequence,
    },
    /// An empty page incorrectly advertised an older page.
    #[error("empty session page has next cursor {next_before}")]
    EmptyPageCursor {
        /// The invalid next cursor.
        next_before: Sequence,
    },
    /// A page cursor did not move toward older rows.
    #[error("session cursor did not advance from {before} to {next_before}")]
    CursorDidNotAdvance {
        /// The cursor used for the read.
        before: Sequence,
        /// The cursor returned by the host.
        next_before: Sequence,
    },
    /// A nonempty page's cursor was newer than its oldest row.
    #[error("next cursor {next_before} is newer than oldest row {oldest}")]
    CursorAfterOldest {
        /// The host-returned cursor.
        next_before: Sequence,
        /// The page's oldest sequence.
        oldest: Sequence,
    },
    /// A host returned more rows than requested.
    #[error("session page returned {actual} rows when at most {requested} were requested")]
    PageTooLarge {
        /// Requested page size.
        requested: usize,
        /// Returned page size.
        actual: usize,
    },
    /// Host-supplied pure snapshots were invalid.
    #[error("invalid team snapshot")]
    Core {
        /// The precise pure-algebra failure.
        #[source]
        source: tinyteams_core::error::Error,
    },
}

/// A runtime result.
pub type Result<T> = std::result::Result<T, Error>;

impl From<tinyteams_core::error::Error> for Error {
    fn from(source: tinyteams_core::error::Error) -> Self {
        Self::Core { source }
    }
}
