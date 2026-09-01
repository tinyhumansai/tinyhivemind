//! Stable records describing the live threads in one desk.

use crate::Sequence;
use serde::{Deserialize, Serialize};

/// One live thread, summarised for a viewer who has not read the desk.
///
/// Everything except [`landed`](Self::landed) is a fold over the transcript.
/// Where a thread's work ended up is board state this crate does not hold, so
/// the host fills that field after the fold returns.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ThreadLine {
    /// Sequence of the message that opened the thread.
    pub root: Sequence,
    /// Opening words of the root, truncated to
    /// [`THREAD_OPENING_CHARS`](super::THREAD_OPENING_CHARS).
    pub opening: String,
    /// Replies counted directly under the root.
    pub replies: usize,
    /// Newest sequence belonging to this thread, including the root itself.
    pub latest: Sequence,
    /// Where the thread's work landed, supplied by the host; `None` until then.
    pub landed: Option<String>,
}
