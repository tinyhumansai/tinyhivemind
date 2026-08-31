//! Stable quorum inputs and standings.

use serde::{Deserialize, Serialize};

use crate::trace::TopicId;

/// When the room is entitled to call a topic settled.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct QuorumPolicy {
    /// Distinct supporters a topic needs to carry.
    pub threshold: u32,
    /// How many sequences back support still counts.
    pub window: u32,
    /// Whether support must cite grounds to count.
    pub require_grounded: bool,
}

impl QuorumPolicy {
    /// A conservative default: two grounded supporters within thirty sequences.
    pub const DEFAULT: Self = Self {
        threshold: 2,
        window: 30,
        require_grounded: true,
    };
}

impl Default for QuorumPolicy {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Where one topic stands in the current window.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct TopicStanding {
    /// The topic being weighed.
    pub topic: TopicId,
    /// Distinct still-counting supporters, in first-support order.
    pub supporters: Vec<String>,
    /// Advocates silenced by a grounded objection, in first-silenced order.
    pub silenced: Vec<String>,
    /// Fixed-point weight of the surviving support.
    pub support: i64,
}

impl TopicStanding {
    /// Return whether this topic has reached `policy.threshold` supporters.
    #[must_use]
    pub fn carried(&self, policy: &QuorumPolicy) -> bool {
        u32::try_from(self.supporters.len()).is_ok_and(|count| count >= policy.threshold)
    }
}

/// What the room's standings add up to.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ConsensusState {
    /// No topic has carried yet.
    Deliberating,
    /// Exactly one topic has carried.
    Quorum {
        /// The topic that carried.
        topic: TopicId,
    },
    /// Two or more topics carried at once, in topic order.
    Deadlocked {
        /// Every topic at or above threshold.
        topics: Vec<TopicId>,
    },
}
