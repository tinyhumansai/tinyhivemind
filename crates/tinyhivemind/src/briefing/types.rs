//! Stable team briefing records.

use crate::{SessionMessage, ThreadLine};
use serde::{Deserialize, Serialize};

/// One teammate described to the initialized viewer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct BriefedTeammate {
    /// Stable agent id and mention handle without the leading `@`.
    pub id: String,
    /// Human-readable agent label.
    pub label: String,
    /// Optional host-supplied team role.
    pub role: Option<String>,
    /// Optional host-supplied agent description.
    pub description: Option<String>,
}

/// Ephemeral context describing one viewer's team and shared desk.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct TeamBriefing {
    /// Agent id receiving the briefing.
    pub viewer_id: String,
    /// Canonical desk id.
    pub desk_id: String,
    /// Operator-facing desk name.
    pub desk_name: String,
    /// Other active teammates in deterministic effective order.
    pub teammates: Vec<BriefedTeammate>,
}

/// One host-supplied block of context that is not in the log and not a thread.
///
/// Board state, open work, attachments — anything a host wants a turn to know
/// and this crate cannot compute. It is carried beside the operator's message,
/// never appended to it, so nothing downstream has to cut it back off before
/// reasoning about what was asked.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct BriefingNote {
    /// Short heading, e.g. `Work raised in this conversation`.
    pub heading: String,
    /// Lines rendered under that heading, in the host's order.
    pub lines: Vec<String>,
}

/// Context for a turn that is neither the team nor the transcript.
///
/// [`threads`](Self::threads) is a fold this crate computes;
/// [`notes`](Self::notes) is whatever the host adds. Both stay separate from
/// [`SessionInitialization::history`], which is the log and only the log.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionContext {
    /// Live threads in this desk, newest activity first.
    pub threads: Vec<ThreadLine>,
    /// Host-supplied blocks this crate cannot compute.
    pub notes: Vec<BriefingNote>,
}

/// Separate ephemeral briefing and durable attributed history for a new turn.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionInitialization {
    /// Ephemeral team context; it is not part of the host log.
    pub briefing: TeamBriefing,
    /// Threads and host notes; typed, and never merged into a message.
    pub context: SessionContext,
    /// Chronological attributed messages from the host log.
    pub history: Vec<SessionMessage>,
}
