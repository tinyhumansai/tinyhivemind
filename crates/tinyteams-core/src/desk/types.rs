//! Host-facing desk overlay records and their stable wire representation.

use serde::{Deserialize, Deserializer, Serialize};

/// How a desk chooses a responder when no active agent is mentioned directly.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ResponderMode {
    /// Choose the first effective active desk member.
    #[default]
    Lead,
    /// Ask the runtime selector when at least two effective members exist.
    Auto,
}

/// A declared or operator-added group conversation.
///
/// Member ids are ordered: the first non-retired member is the desk lead
/// unless a complete [`DeskOrder`] replaces that order.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Desk {
    /// The opaque, case-sensitive desk identifier.
    pub id: String,
    /// The operator-facing display name and exact lookup alias.
    pub name: String,
    /// Optional operator-facing context about the desk.
    #[serde(deserialize_with = "deserialize_required_description")]
    pub description: Option<String>,
    /// Founding agent ids in their declared order.
    pub members: Vec<String>,
    /// The desk's responder policy; omitted for the backward-compatible lead mode.
    #[serde(default, skip_serializing_if = "is_lead_mode")]
    pub responder_mode: ResponderMode,
}

/// One host-owned membership addition applied after founding members.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DeskMember {
    /// The exact id of the desk receiving the member.
    pub desk_id: String,
    /// The agent id to append if it has not already appeared.
    pub agent_id: String,
}

/// A complete replacement order for one desk's final member set.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DeskOrder {
    /// The exact id of the desk being ordered.
    pub desk_id: String,
    /// Every final member id exactly once, in the desired order.
    pub ordered: Vec<String>,
}

fn deserialize_required_description<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)
}

const fn is_lead_mode(mode: &ResponderMode) -> bool {
    matches!(mode, ResponderMode::Lead)
}
