//! Stable trace payloads deposited in the shared transcript.

use serde::{Deserialize, Deserializer, Serialize};
use tinyteams::{SessionAuthor, Sequence};

/// A proposal identity that support and objection attach to.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct TopicId(pub String);

impl TopicId {
    /// Borrow the identity as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TopicId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

/// What one trace does to the deliberation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TraceKind {
    /// Puts a new option on the floor.
    Propose,
    /// Adds the author to a topic's supporter set.
    Support,
    /// Silences the advocate of the targeted message.
    Object,
    /// Supplies grounds without taking a position.
    Evidence,
    /// Asks for something the room has not established.
    Question,
    /// Records the decision after quorum.
    Commit,
}

/// One typed deposit read out of an authored message.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Trace {
    /// Host sequence of the message that carried this trace.
    pub sequence: Sequence,
    /// Preserved author of that message.
    pub author: SessionAuthor,
    /// What the trace does.
    pub kind: TraceKind,
    /// Topic the trace attaches to, when it names one.
    #[serde(deserialize_with = "deserialize_required_option")]
    pub topic: Option<TopicId>,
    /// Message an objection is aimed at.
    #[serde(deserialize_with = "deserialize_required_option")]
    pub target: Option<Sequence>,
    /// Sequences cited as grounds, in authored order.
    pub cites: Vec<Sequence>,
    /// Exact authored marker line.
    pub text: String,
    /// UTF-8 byte offset of the marker within the message body.
    pub offset: usize,
}

impl Trace {
    /// Return whether the trace carries grounds.
    ///
    /// A conclusion offered without grounds is second-class: an information
    /// cascade forms precisely because only conclusions are public, so an
    /// ungrounded position cannot move a quorum that requires grounding.
    #[must_use]
    pub fn grounded(&self) -> bool {
        !self.cites.is_empty()
    }

    /// Return the author's canonical agent id, when an agent authored it.
    #[must_use]
    pub fn agent_id(&self) -> Option<&str> {
        match &self.author {
            SessionAuthor::Agent { id, .. } => Some(id.as_str()),
            SessionAuthor::Operator
            | SessionAuthor::Person { .. }
            | SessionAuthor::System { .. } => None,
        }
    }
}

fn deserialize_required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}
