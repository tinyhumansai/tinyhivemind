//! Stable episode policy, state, and step outcomes.

use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    attention::{AgentThreshold, BidReason},
    directory::DirectoryPolicy,
    quorum::{QuorumPolicy, TopicStanding},
    salience::SalienceWeights,
    trace::TopicId,
};
use tinyhivemind::{Conversation, Sequence};

/// The width [`EpisodePolicy::DEFAULT`] runs rounds at.
///
/// Not `1`. A seat is an async session and the default should say so; the
/// benchmark's arms measure what it costs rather than assuming it is free.
/// Four is the width at which a room of five still leaves somebody to answer
/// in the next round, which is the smallest width that is meaningfully
/// concurrent without being a broadcast.
pub const DEFAULT_ROUND_WIDTH: u32 = 4;

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
    /// Turns one round may authorize to run concurrently.
    ///
    /// A bound, not a quota: a bid still has to clear its own threshold to be
    /// in a round at all, so a round is often narrower than this and is never
    /// wider. `1` is the sequential episode and reproduces every number
    /// recorded before rounds existed, bit for bit. `0` is
    /// [`Error::ZeroRoundWidth`] at step time, on the `defer_cap` precedent.
    ///
    /// Widening a round does not buy turns — `turn_budget` still bounds the
    /// total — it spends them in fewer, deeper rounds. What it buys is depth,
    /// and what it costs is that the members in one round cannot read each
    /// other. See `docs/specs/concurrent-rounds.md`.
    ///
    /// [`Error::ZeroRoundWidth`]: crate::error::Error::ZeroRoundWidth
    pub round_width: u32,
    /// Whether the opening round is blind.
    pub blind_round: bool,
    /// Percent of grounded share above which a member is damped.
    pub dominance_cap: u32,
    /// Distinct supporters after which restating a topic scores nothing.
    pub repetition_cap: u32,
    /// How to read the transcript for who knows what, if at all.
    ///
    /// `None` folds no directory, leaves [`BidReason::Knows`] unreachable, and
    /// is what [`EpisodePolicy::DEFAULT`] carries. It is off for the same
    /// reason `refutation_cap` is: the benchmark has not scored the arm yet,
    /// and two mechanisms in this crate have already been measured, lost, and
    /// been left opt-in rather than quietly defaulted on. See
    /// `docs/adr/0007-the-directory-is-folded-from-citations.md`.
    #[serde(deserialize_with = "deserialize_required_directory")]
    pub directory: Option<DirectoryPolicy>,
    /// Live deferrals after which a deferred topic stops being promoted.
    ///
    /// `None` leaves the chain bounded only by `turn_budget`. `Some(0)` is
    /// [`Error::ZeroDeferCap`] at step time: it would cap the mechanism before
    /// anyone could use it, which is a configuration error rather than a
    /// quieter way of switching it off.
    ///
    /// [`Error::ZeroDeferCap`]: crate::error::Error::ZeroDeferCap
    #[serde(deserialize_with = "deserialize_required_defer_cap")]
    pub defer_cap: Option<u32>,
    /// When a topic is entitled to carry.
    pub quorum: QuorumPolicy,
    /// Salience weights.
    pub weights: SalienceWeights,
}

/// Require `directory` to be written out, even when it is `null`.
///
/// The convention `refutation_cap` established: an absent key would silently
/// mean "off", and off is the shipping setting, so a host that means to leave
/// the mechanism off has to say so rather than inherit it. An
/// [`EpisodePolicy`] serialized before this field existed fails to decode
/// rather than quietly acquiring a default.
fn deserialize_required_directory<'de, D>(
    deserializer: D,
) -> Result<Option<DirectoryPolicy>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<DirectoryPolicy>::deserialize(deserializer)
}

/// Require `defer_cap` to be written out, even when it is `null`.
fn deserialize_required_defer_cap<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<u32>::deserialize(deserializer)
}

impl EpisodePolicy {
    /// A conservative default.
    ///
    /// The budget is deliberately small. Conformity in a group of language
    /// models rises with interaction time, so a long episode buys correlated
    /// error rather than better judgement.
    ///
    /// Expert delegation is off: `directory` and `defer_cap` are both `None`
    /// pending the benchmark arm that scores them.
    pub const DEFAULT: Self = Self {
        turn_budget: 12,
        round_width: DEFAULT_ROUND_WIDTH,
        blind_round: true,
        dominance_cap: 50,
        repetition_cap: 3,
        directory: None,
        defer_cap: None,
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

/// One turn a round authorizes.
///
/// A turn is what one member may do. What the *episode* becomes is carried once
/// by the round, in [`HiveStep::Speak::next_state`], because `spent` advances by
/// the round's width and every speaker in it is charged.
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
    /// Exclusive lower bound: the sequence the episode opened at.
    ///
    /// Carried on the turn because [`project_for`] needs it and the state it
    /// used to be read from now belongs to the round rather than the turn.
    ///
    /// [`project_for`]: crate::episode::project_for
    pub watermark: Sequence,
    /// The sequence this round was folded at.
    ///
    /// A peer's row above this was written *concurrently* with this turn, so
    /// this turn cannot have read it and [`project_for`] withholds it. At
    /// `round_width: 1` nothing is ever above it and the clause is inert,
    /// which is what makes a round of one bit-identical rather than merely
    /// equivalent.
    ///
    /// [`project_for`]: crate::episode::project_for
    pub round_start: Sequence,
}

/// The outcome of one episode step.
///
/// [`Self::Speak`] carries a **round**: the turns authorized to run
/// concurrently, and the one state the episode takes once all of them are
/// appended. See `docs/specs/concurrent-rounds.md` and ADR 0014.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum HiveStep {
    /// A round of members takes the floor together.
    Speak {
        /// The authorized turns: non-empty, in desk order, and never more than
        /// `policy.round_width` of them.
        ///
        /// A host may run fewer than it was handed — a seat can be
        /// unavailable — but it commits `next_state` only once it has appended
        /// all of them. Committing after a subset would charge a threshold
        /// nobody spent.
        turns: Vec<HiveTurn>,
        /// State to commit once every turn in the round is durably appended.
        next_state: Box<EpisodeState>,
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
