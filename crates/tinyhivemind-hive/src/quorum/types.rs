//! Stable quorum inputs and standings.

use serde::{Deserialize, Deserializer, Serialize};

use crate::trace::TopicId;

/// Require `refutation_cap` to be written out, even when it is `null`.
///
/// An absent key would silently mean "off", and "off" is the setting the
/// benchmark chose. A host that means to leave it off should have to say so,
/// the same way `Trace::topic` and `DispatchConversation::thread_root` are
/// required-but-nullable rather than defaulted.
fn deserialize_required_cap<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<u32>::deserialize(deserializer)
}

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
    /// Distinct grounded refuters that cap a topic out of contention, if any.
    ///
    /// A refutation argues a cited fact against the topic itself, so unlike an
    /// objection it does not need to name every advocate in turn. `None` still
    /// *records* refutations in [`TopicStanding::refuted_by`]; it declines to
    /// let them cap anything.
    ///
    /// It is `None` by default, and that is an empirical result rather than
    /// caution. On the benchmark's task the mechanism costs accuracy, and the
    /// cost grows with how noisy each member's private read is — because a
    /// refutation is *global* where an objection is local, so a member firing
    /// one on a noisy read removes an option for the whole room. See
    /// `docs/experiments/2026-09-01-refutation-and-grounds.md`.
    #[serde(deserialize_with = "deserialize_required_cap")]
    pub refutation_cap: Option<u32>,
    /// Whether support must trace back to a stated fact, not merely to a
    /// citation.
    ///
    /// A support citing another support is a citation of an opinion, which is
    /// the information-cascade condition with a citation on it. When set, only
    /// support whose citation chain reaches a [`TraceKind::Evidence`] counts,
    /// and an objection silences nobody unless its author has deposited
    /// evidence in the window. Implies `require_grounded`.
    ///
    /// [`TraceKind::Evidence`]: crate::trace::TraceKind::Evidence
    pub require_evidential: bool,
}

impl QuorumPolicy {
    /// A conservative default: two grounded supporters within thirty sequences.
    ///
    /// Both of the narrowing knobs are off, and both for the same reason: the
    /// benchmark scored them and they lost. A default is not the place to
    /// carry a mechanism its own harness says costs accuracy.
    pub const DEFAULT: Self = Self {
        threshold: 2,
        window: 30,
        require_grounded: true,
        refutation_cap: None,
        require_evidential: false,
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
    /// Distinct members who refuted this topic, in first-refutation order.
    ///
    /// Nothing is removed when a topic is refuted. It keeps its supporters and
    /// its weight and stays in the standings, so the transcript records that
    /// the room considered it and a reader can audit the refutation back to the
    /// message it cites.
    pub refuted_by: Vec<String>,
    /// Fixed-point weight of the surviving support.
    pub support: i64,
}

impl TopicStanding {
    /// Return whether this topic has reached `policy.threshold` supporters and
    /// has not been capped by `policy.refutation_cap` distinct refuters.
    ///
    /// The refutation check is a cap rather than a debit. `carried` reads the
    /// supporter *count*, not the weight, so subtracting from `support` would
    /// change nothing; capping is the only shape that expresses "this
    /// hypothesis is dead regardless of how many members like it".
    #[must_use]
    pub fn carried(&self, policy: &QuorumPolicy) -> bool {
        if let Some(cap) = policy.refutation_cap
            && u32::try_from(self.refuted_by.len()).is_ok_and(|count| count >= cap)
        {
            return false;
        }
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
