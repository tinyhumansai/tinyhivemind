//! Stable host-facing session records.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A monotonically increasing address in the host-owned session log.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Sequence(pub u64);

impl fmt::Display for Sequence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// The desk and optional thread viewed by one turn.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Conversation {
    /// Canonical case-sensitive desk id.
    pub desk_id: String,
    /// Operator-facing desk display name.
    pub desk_name: String,
    /// Root sequence for a thread, or `None` for the desk channel.
    pub thread_root: Option<Sequence>,
}

/// The preserved author of a host log row.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionAuthor {
    /// A local operator-authored message.
    Operator,
    /// A human participant.
    Person {
        /// Stable person id.
        id: String,
        /// Display label captured with the row.
        label: String,
    },
    /// An agent participant.
    Agent {
        /// Stable agent id.
        id: String,
        /// Display label captured with the row.
        label: String,
    },
    /// A system or workflow source.
    System {
        /// Host-neutral system category.
        kind: String,
        /// Display label captured with the row.
        label: String,
    },
}

/// One raw row borrowed from the host-owned log.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct LogMessage {
    /// Global host sequence.
    pub sequence: Sequence,
    /// Stored desk/chat spelling; `None` is General.
    pub chat_id: Option<String>,
    /// Direct parent sequence, if this is a thread reply.
    pub parent: Option<Sequence>,
    /// Preserved row author.
    pub author: SessionAuthor,
    /// Exact authored content.
    pub content: String,
}

/// One newest-first page returned by [`super::SessionLog`].
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionPage {
    /// Rows in strictly descending sequence order.
    pub messages: Vec<LogMessage>,
    /// Exclusive cursor for an older page, equal to the oldest row when set.
    pub next_before: Option<Sequence>,
}

/// One chronological, attributed message presented to an agent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionMessage {
    /// Original host sequence.
    pub sequence: Sequence,
    /// Original author, never collapsed into the viewer.
    pub author: SessionAuthor,
    /// Original nonblank content.
    pub content: String,
}

/// Parameters for one bounded transcript projection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionQuery {
    /// Desk and optional thread to project.
    pub conversation: Conversation,
    /// Exclusive initial upper bound, often the triggering message sequence.
    pub before: Option<Sequence>,
    /// Maximum number of qualifying messages returned.
    pub window: usize,
}
