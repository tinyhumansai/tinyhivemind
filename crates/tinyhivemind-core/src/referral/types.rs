//! Stable payloads for one bounded cross-desk referral decision.

use crate::{
    dispatch::DispatchConversation,
    dispatch::DispatchKey,
    dispatch::{HOP_BUDGET_SPENT, NO_AVAILABLE_TARGET},
    mention::Mention,
};
use serde::{Deserialize, Serialize};
use std::fmt;

/// How far a referral may travel from the conversation that triggered it.
///
/// The three settings widen strictly, which is why this is one knob rather
/// than two: a desk mention only means anything once a turn is allowed to run
/// somewhere other than here, so `Desks` without `Channels` is not a policy a
/// host could hold.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferralReach {
    /// Stay on this conversation, exactly as
    /// [`mention_dispatch`](crate::dispatch::mention_dispatch) does.
    #[default]
    Local,
    /// A target who is not on this desk runs on their own desk instead of
    /// being pulled into this conversation.
    Channels,
    /// The same, and a nonquiet `@#desk` mention selects that desk's one
    /// responder.
    Desks,
}

impl ReferralReach {
    /// Whether a turn may run on a conversation other than the trigger's.
    #[must_use]
    pub const fn crosses(self) -> bool {
        matches!(self, Self::Channels | Self::Desks)
    }

    /// Whether a desk mention is a candidate at all.
    #[must_use]
    pub const fn addresses_desks(self) -> bool {
        matches!(self, Self::Desks)
    }
}

/// Explicit host policy for cross-desk referral.
///
/// [`Self::DEFAULT`] refers nothing, so a host that does not opt in gets no
/// referrals. With only `enabled` and `max_hops` set, the decision is exactly
/// the one [`crate::dispatch::mention_dispatch`] makes, on the same
/// conversation.
///
/// Note what is deliberately absent: a bound on how *many* channels one desk
/// may ask. `max_hops` bounds the depth of a chain, and bounding its width is
/// the host's job, because only the host knows what a question costs it.
///
/// # What a bad width bound costs, measured
///
/// That is the right division of labour and it is also a trap, so the number
/// belongs here rather than only in the host that found it. The obvious host
/// bound — *every peer, once* — is not a bound at all: it grows with the
/// federation while the asking desk's turn budget stays whatever its own size
/// earned. A desk of ten members has thirty turns; at fifty desks it may spend
/// forty-nine of them asking.
///
/// The benchmark's federation does exactly that, and the collapse is total.
/// Correct on 100% of federations at twelve desks, **0.0% at twenty-five and
/// above**, with every desk episode ending exhausted — two thousand of two
/// thousand at a hundred desks. Not deadlocked and not idle: spent. The
/// federation talks itself to death.
///
/// Bounding width at a small constant instead — two questions per desk,
/// whatever the federation's size — restores 100% and costs *less*: 1343 turns
/// against 6624 at a hundred desks, because the turns the unbounded arm spent
/// asking are the turns it needed for deciding. See
/// `docs/experiments/2026-09-10-hive-at-scale.md`.
///
/// Two rules follow, and they are what a host should take from this:
///
/// 1. **Bound width by a constant the host states, never by the number of
///    peers.** A bound that scales with the federation prices nothing.
/// 2. **Do not spend an authorized turn on the question.** A question is a
///    model call, not a turn, and charging it to the floor takes the budget
///    out of the room that has to decide. This is
///    [ADR 0012](https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/adr/0012-an-exchange-round-spends-model-calls-not-turns.md)
///    one level up.
///
/// A bounded question also has a ceiling worth knowing about: it cannot
/// recover an error the whole federation shares, because the peer it asks
/// holds the same error. At a hundred desks sharing seven blind spots every
/// bounded pairwise arm scores zero. What lifts that is a desk publishing its
/// reading to *every* channel — one model call, not one per peer — which is a
/// different mechanism and not more of this one.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ReferralPolicy {
    /// Whether referral is enabled for this committed reply.
    pub enabled: bool,
    /// Maximum permitted chain depth, supplied by the host without a library cap.
    pub max_hops: u32,
    /// How far a child turn may travel from the triggering conversation.
    pub reach: ReferralReach,
    /// Whether a reply committed under a crossing referral may carry one
    /// answer back to the conversation that asked.
    pub returns: bool,
}

impl ReferralPolicy {
    /// Referral fully disabled: the conservative default a host must opt out of.
    pub const DEFAULT: Self = Self {
        enabled: false,
        max_hops: 0,
        reach: ReferralReach::Local,
        returns: false,
    };
}

impl Default for ReferralPolicy {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Where a crossing referral came from, so its answer can be carried back.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ReferralOrigin {
    /// The conversation that asked.
    pub conversation: DispatchConversation,
    /// The exact agent id that asked.
    pub asker_id: String,
}

/// Inputs captured from one committed agent reply.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ReferralInput {
    /// Idempotency key for the committed reply.
    pub key: DispatchKey,
    /// Conversation the reply was committed on.
    pub conversation: DispatchConversation,
    /// Exact active agent id that authored the reply.
    pub author_id: String,
    /// Exact committed reply content passed to the child turn.
    pub content: String,
    /// Revalidated resolved mentions from the committed content.
    pub mentions: Vec<Mention>,
    /// Current chain depth of the committed reply.
    pub hop: u32,
    /// The referral this reply is answering, when it is answering one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<ReferralOrigin>,
}

/// Which direction a referral travels.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReferralKind {
    /// An outbound question, selected by a mention.
    Forward,
    /// The one answer carried back to the conversation that asked.
    Return,
}

/// Why no child turn was selected.
///
/// The variant is the operator's record and `{:?}` prints it. The `Display`
/// rendering is the acting agent's, and is deliberately coarser: every reason
/// that turns on whether a named *agent* exists, is active, or is reachable
/// renders as [`NO_AVAILABLE_TARGET`]. A reason about a *desk* renders its own
/// wording, because a desk snapshot carries no per-viewer scoping and so has
/// no hidden state a refusal could disclose. See [ADR 0009].
///
/// [ADR 0009]: https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/adr/0009-a-refusal-renders-what-the-caller-already-holds.md
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoReferralReason {
    /// Host policy explicitly disabled referral.
    Disabled,
    /// The current reply has exhausted the host's hop budget.
    HopLimitReached,
    /// The committed author is not an active agent.
    SourceInactive,
    /// Neither a forward candidate nor an available return was present.
    NoReferralTarget,
    /// The first candidate addresses the author.
    SelfMention,
    /// The first candidate addresses an inactive agent.
    TargetInactive,
    /// The first candidate addresses the desk the reply was committed on.
    SelfDesk,
    /// The addressed desk has no eligible member other than the author.
    EmptyDesk,
    /// The target is active but belongs to no desk, so it has nowhere to run.
    TargetDeskless,
    /// The addressed desk could not be resolved against the snapshot.
    UnknownDesk,
    /// The child hop could not be represented.
    HopOverflow,
}

impl fmt::Display for NoReferralReason {
    /// Render the sentence an acting agent may repeat to a person.
    ///
    /// A reason renders wording of its own only when what it reveals is
    /// something the caller already holds: the policy it supplied, the hop it
    /// supplied, its own identity, its own desk, or the desk directory that is
    /// the same for every viewer. Anything that turns on another agent's
    /// existence, activity or desk membership renders as
    /// [`NO_AVAILABLE_TARGET`].
    ///
    /// [`Self::NoReferralTarget`] is withheld even though some of its paths
    /// are purely local — a policy that refuses returns, an input carrying no
    /// origin — because one of them is "nothing addressable was mentioned",
    /// and a variant is classified by its most disclosing path.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Disabled => "referring this to another desk is turned off here",
            Self::HopLimitReached | Self::HopOverflow => HOP_BUDGET_SPENT,
            Self::SourceInactive => {
                "the agent that wrote this is no longer active, so nothing was passed on"
            }
            Self::SelfMention => "an agent cannot pass a message on to itself",
            Self::SelfDesk => "this is already on the desk it was addressed to",
            Self::UnknownDesk => "no single desk goes by that name",
            Self::NoReferralTarget
            | Self::TargetInactive
            | Self::EmptyDesk
            | Self::TargetDeskless => NO_AVAILABLE_TARGET,
        })
    }
}

/// One canonical child-turn referral.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Referral {
    /// Idempotency key derived from the committed reply.
    pub key: DispatchKey,
    /// Which direction it travels.
    pub kind: ReferralKind,
    /// Exact active author id.
    pub source_id: String,
    /// Exact active target id.
    pub target_id: String,
    /// Exact committed reply content.
    pub content: String,
    /// The conversation the trigger was committed on.
    pub from: DispatchConversation,
    /// The conversation the child turn runs on.
    pub to: DispatchConversation,
    /// Where to carry an answer back to, set only on a crossing forward.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<ReferralOrigin>,
    /// Checked child depth.
    pub child_hop: u32,
}

impl Referral {
    /// Whether this referral leaves the conversation that triggered it.
    #[must_use]
    pub fn crosses(&self) -> bool {
        self.from != self.to
    }
}

/// Pure result of considering one committed reply.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReferralDecision {
    /// No child turn may be attempted.
    None {
        /// Deterministic reason for stopping.
        reason: NoReferralReason,
    },
    /// Exactly one canonical child turn may be attempted.
    One {
        /// Referral to pass unchanged to the host queue.
        referral: Box<Referral>,
    },
}
