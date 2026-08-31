//! Stable host-facing roster record types.

use serde::{Deserialize, Deserializer, Serialize};

/// One agent that may participate in a shared conversation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct RosterMember {
    /// The opaque, case-sensitive agent identifier.
    pub id: String,
    /// An optional display name that can also be mentioned.
    #[serde(deserialize_with = "deserialize_required_name")]
    pub name: Option<String>,
}

/// One human participant visible to agents in a shared conversation.
///
/// `Person` intentionally avoids a host-specific `User` type: hosts project
/// whichever human identity they use into this neutral boundary record.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Person {
    /// The opaque, case-sensitive person identifier.
    pub id: String,
    /// The authored display label and source for a stable mention slug.
    pub label: String,
}

fn deserialize_required_name<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)
}
