//! Stable episode policy, state, and step outcomes.

use serde::{Deserialize, Serialize};

use crate::{
    attention::{AgentThreshold, BidReason},
    quorum::{QuorumPolicy, TopicStanding},
    salience::SalienceWeights,
    trace::TopicId,
};
use tinyteams::{Conversation, Sequence};

/// Which class of turn the episode is taking.
///
/// The transition from [`Phase::Deliberate`] to [`Phase::Commit`] is one-way.
/// Deliberation and commitment are different kinds of turn, and a room that has
/// settled does not reopen because a late trace arrived.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Proposals are still on the floor.
    #[default]
    Deliberate,
    /// Quorum was reached; the room is recording its decision.
    Commit,
}

/// How much of the shared transcript one turn may see.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// Peer agent messages from this episode are hidden.
    ///
    /// This is the round that restores independence. A shared transcript
    /// destroys it — the third speaker reads the first two before it answers —
    /// and a first position formed without sight of peers is the cheapest
    /// available repair. It costs a projection flag rather than concurrency.
    Blind,
    /// The full projected transcript.
    Full,
}

/// Everything the episode is bounded and tuned by.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct EpisodePolicy {
    /// Hard cap on turns. Finite, so an episode always terminates.
    pub turn_budget: u32,
    /// Whether the opening round is blind.
    pub blind_round: bool,
    /// Percent of grounded share above which a member is damped.
    pub dominance_cap: u32,
    /// Distinct supporters after which restating a topic scores nothing.
    pub repetition_cap: u32,
    /// When a topic is entitled to carry.
    pub quorum: QuorumPolicy,
    /// Salience weights.
    pub weights: SalienceWeights,
}

impl EpisodePolicy {
    /// A conservative default.
    ///
    /// The budget is deliberately small. Conformity in a group of language
    /// models rises with interaction time, so a long episode buys correlated
    /// error rather than better judgement.
    pub const DEFAULT: Self = Self {
        turn_budget: 12,
        blind_round: true,
        dominance_cap: 50,
        repetition_cap: 3,
        quorum: QuorumPolicy::DEFAULT,
        weights: SalienceWeights::DEFAULT,
    };
}

impl Default for EpisodePolicy {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The caller-owned, committable state of one episode.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct EpisodeState {
    /// Desk and optional thread the episode runs on.
    pub conversation: Conversation,
    /// Turns already taken.
    pub spent: u32,
    /// Current phase.
    pub phase: Phase,
    /// Per-member thresholds, carried across turns.
    pub thresholds: Vec<AgentThreshold>,
    /// Exclusive lower bound: the sequence the episode opened at.
    pub watermark: Sequence,
    /// The sequence standings were folded to when the phase first flipped to
    /// [`Phase::Commit`].
    ///
    /// `None` until that flip happens, then fixed for the rest of the
    /// episode. A `!commit` trace only counts toward [`HiveStep::Converged`]
    /// when its sequence is strictly greater than this boundary — otherwise a
    /// trace that merely happens to share the carried topic and predates the
    /// commit turn being authorized could be misread as evidence that turn
    /// recorded a decision.
    ///
    /// [`HiveStep::Converged`]: crate::episode::HiveStep::Converged
    pub commit_boundary: Option<Sequence>,
}

impl EpisodeState {
    /// Open an episode on a conversation at a watermark.
    #[must_use]
    pub fn opened(conversation: Conversation, watermark: Sequence) -> Self {
        Self {
            conversation,
            spent: 0,
            phase: Phase::Deliberate,
            thresholds: Vec::new(),
            watermark,
            commit_boundary: None,
        }
    }
}

/// The single turn an episode step authorizes.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct HiveTurn {
    /// The member taking the floor.
    pub agent_id: String,
    /// Which class of turn it is.
    pub phase: Phase,
    /// How much of the transcript this turn may see.
    pub visibility: Visibility,
    /// Why this member won the floor.
    pub reason: BidReason,
    /// State to commit once the turn is durably appended.
    pub next_state: EpisodeState,
}

/// The outcome of one episode step.
///
/// There is deliberately no variant carrying more than one turn.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum HiveStep {
    /// Exactly one member takes the floor.
    Speak {
        /// The authorized turn.
        turn: Box<HiveTurn>,
    },
    /// One topic carried and the room has recorded it.
    Converged {
        /// The topic that carried.
        topic: TopicId,
        /// The standing that carried it.
        standing: Box<TopicStanding>,
    },
    /// Two or more topics carried and nobody can break the tie.
    Deadlocked {
        /// Every tied topic.
        topics: Vec<TopicId>,
    },
    /// The turn budget is spent.
    Exhausted {
        /// Turns taken.
        spent: u32,
    },
    /// Nobody's urge cleared their threshold.
    Idle,
}
