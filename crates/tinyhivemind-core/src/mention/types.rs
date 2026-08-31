//! Stable mention payloads shared with hosts.

use serde::{Deserialize, Serialize};

/// The identity addressed by an authored mention.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MentionTarget {
    /// One agent roster member.
    Agent {
        /// The exact agent id.
        id: String,
    },
    /// One human participant.
    Person {
        /// The exact person id.
        id: String,
    },
    /// The active members of one desk.
    Desk {
        /// The exact desk id.
        id: String,
    },
    /// The addressed desk, or the full active roster in General.
    Everyone,
}

/// One resolved mention and its exact authored source span.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Mention {
    /// The resolved or preserved target.
    pub target: MentionTarget,
    /// The exact authored mention text, including `@` or `@#`.
    pub text: String,
    /// The UTF-8 byte offset of `text` in the message body.
    pub offset: usize,
    /// Whether this mention supplies context without pinging or routing.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub quiet: bool,
}

/// The identity that authored a message containing mentions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MentionAuthor {
    /// An agent authored the message.
    Agent {
        /// The exact agent id.
        id: String,
    },
    /// A person authored the message.
    Person {
        /// The exact person id.
        id: String,
    },
    /// The host cannot classify the author as a roster identity.
    Other,
}
