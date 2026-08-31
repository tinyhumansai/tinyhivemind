//! Stable team briefing records.

use crate::SessionMessage;
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

/// Separate ephemeral briefing and durable attributed history for a new turn.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionInitialization {
    /// Ephemeral team context; it is not part of the host log.
    pub briefing: TeamBriefing,
    /// Chronological attributed messages from the host log.
    pub history: Vec<SessionMessage>,
}
