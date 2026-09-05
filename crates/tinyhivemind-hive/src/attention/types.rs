//! Stable attention-market inputs and bids.

use serde::{Deserialize, Serialize};

use crate::{
    quorum::TopicStanding,
    salience::SalienceWeights,
    trace::{TopicId, Trace},
};
use tinyhivemind::Sequence;

/// One member's standing willingness to take the floor.
///
/// `threshold` is the only field this crate writes: taking the floor raises it
/// and staying quiet lowers it, which rotates the floor around the desk. It is
/// not specialisation, and nothing here reinforces it per topic.
///
/// `affinity` is a **host-supplied prior** — a diffuse cue, in Hollingshead's
/// sense, of the kind a role label carries. It is read and never written. The
/// estimate that is actually earned from the transcript lives in
/// [`Directory`], which folds grounded deposits and the citations they drew
/// into a per-topic weight; `affinity` enters that fold as one term among
/// three rather than as an authority.
///
/// [`Directory`]: crate::directory::Directory
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentThreshold {
    /// Canonical agent id.
    pub agent_id: String,
    /// Urge a bid must reach before this member speaks at all.
    pub threshold: i64,
    /// Per-topic relevance in `0..=100`, in first-declared order.
    pub affinity: Vec<(TopicId, u8)>,
}

impl AgentThreshold {
    /// Build a threshold record with no topical affinity.
    #[must_use]
    pub fn new(agent_id: impl Into<String>, threshold: i64) -> Self {
        Self {
            agent_id: agent_id.into(),
            threshold,
            affinity: Vec::new(),
        }
    }

    /// Return this member's declared relevance for a topic.
    ///
    /// A member with no declared affinity is neutral rather than uninterested,
    /// so an unconfigured roster still deliberates.
    #[must_use]
    pub fn relevance(&self, topic: Option<&TopicId>) -> u8 {
        const NEUTRAL: u8 = 50;
        let Some(topic) = topic else { return NEUTRAL };
        self.affinity
            .iter()
            .find(|(declared, _)| declared == topic)
            .map_or(NEUTRAL, |(_, relevance)| *relevance)
    }
}

/// Why a member wants the floor.
///
/// Ordered by precedence: a member that is both addressed and quiet bids
/// [`BidReason::Addressed`].
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BidReason {
    /// A trace cited or objected to one of this member's messages.
    Addressed,
    /// The room is deadlocked and this member has backed neither side.
    Dissent,
    /// The equality guard lifted the least-heard member.
    Quiet,
    /// Ordinary pull from the salience field.
    Salience,
}

/// One member's bid for the floor.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Bid {
    /// Canonical agent id.
    pub agent_id: String,
    /// Fixed-point urge in thousandths, net of the member's own threshold.
    pub urge: i64,
    /// The dominant reason for the bid.
    pub reason: BidReason,
}

/// Everything the attention market reads, borrowed from the caller.
#[derive(Clone, Copy, Debug)]
pub struct BidContext<'a> {
    /// Traces folded from the projected transcript.
    pub traces: &'a [Trace],
    /// Current standings, used for dissent and repetition.
    pub standings: &'a [TopicStanding],
    /// Active desk members, in desk order. Ties break by this order.
    pub members: &'a [&'a str],
    /// Per-member thresholds and affinities.
    pub thresholds: &'a [AgentThreshold],
    /// The sequence the room is deciding at.
    pub at: Sequence,
    /// Salience weights.
    pub weights: &'a SalienceWeights,
    /// Percent of grounded share above which a member is damped.
    pub dominance_cap: u32,
    /// Distinct supporters after which restating a topic scores nothing.
    pub repetition_cap: u32,
    /// How many sequences back to measure share over.
    pub window: u32,
}
