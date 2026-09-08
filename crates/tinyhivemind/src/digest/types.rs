//! Stable host-owned values for channel compaction.

use crate::{Conversation, Sequence, SessionMessage};
use serde::{Deserialize, Serialize};

/// How much of a channel is kept verbatim, and how much is folded.
///
/// The three numbers are one budget seen from three sides. `keep_live` is the
/// tail no fold may touch, because the newest rows are the ones a turn is
/// answering. `fold_after` is the slack behind that tail: folding on every
/// single row would spend a model call per turn to move one message, so the
/// fold waits until it is worth making. `input_limit` bounds one fold, which
/// is what keeps a first fold over a long history from being a single
/// enormous call — the fold advances in steps and catches up.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DigestPolicy {
    /// Newest rows never folded, always delivered verbatim.
    pub keep_live: usize,
    /// Rows that must accumulate behind the live tail before a fold is worth a call.
    pub fold_after: usize,
    /// Most rows handed to one digester call.
    pub input_limit: usize,
    /// Most characters a digest may occupy.
    pub budget_chars: usize,
}

impl DigestPolicy {
    /// A tail of thirty rows, folded in steps of sixty, into four thousand
    /// characters.
    ///
    /// `keep_live` matches [`SESSION_WINDOW`](crate::SESSION_WINDOW): what a
    /// turn is handed verbatim is what a turn is projected. `fold_after` is
    /// deliberately smaller than `keep_live`, so a room that is moving folds
    /// roughly once every twenty turns rather than once per turn.
    pub const DEFAULT: Self = Self {
        keep_live: 30,
        fold_after: 20,
        input_limit: 60,
        budget_chars: 4000,
    };
}

impl Default for DigestPolicy {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// A bounded, superseding account of one channel's older rows.
///
/// This is host state, not a log row. It replaces its own previous version
/// rather than accumulating, which is exactly why it cannot live in an
/// append-only journal — see `docs/specs/thoughts-and-channels.md`.
///
/// Its content is derived and lossy. Every row it stands for is still in the
/// log at its original sequence, so a reader that needs the words rather than
/// the gist can search for them.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ChannelDigest {
    /// The channel this account is of.
    pub conversation: Conversation,
    /// Inclusive newest sequence the account covers.
    pub through: Sequence,
    /// How many rows have been folded into it across every generation.
    pub covered: u64,
    /// How many times it has been rewritten. A fresh account is generation 1.
    pub generation: u32,
    /// The account itself.
    pub text: String,
}

/// What the next fold of a channel should be.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DigestPlan {
    /// The live tail is short enough; the existing account still stands.
    Current,
    /// Fold rows after `after` through `through` into the account.
    Fold {
        /// Exclusive lower bound: the account already covers this and older.
        after: Option<Sequence>,
        /// Inclusive upper bound for this step.
        through: Sequence,
    },
}

/// Everything one digester call is given.
///
/// It carries no host handle and no callback: a digester is handed a bounded
/// set of rows and the account it is superseding, and returns text.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DigestRequest {
    /// The channel being folded.
    pub conversation: Conversation,
    /// The account this call supersedes, if there is one.
    pub prior: Option<String>,
    /// Rows to fold, chronological, desk-visible only.
    pub messages: Vec<SessionMessage>,
    /// Inclusive newest sequence these rows reach.
    pub through: Sequence,
    /// Most characters the returned account may occupy.
    pub budget_chars: usize,
}

/// Why a digester's output could not become the new account.
///
/// A rejection is not an error: the previous account still stands and the
/// live tail still carries every row the fold would have covered, so a host
/// that ignores this loses nothing but the compaction.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DigestRejection {
    /// The digester returned nothing but whitespace.
    Empty,
    /// The digester overran its stated budget.
    TooLarge {
        /// Characters allowed.
        limit: usize,
        /// Characters returned.
        actual: usize,
    },
    /// The proposed account covered no more than the one it replaces.
    Regressed {
        /// Coverage proposed.
        through: Sequence,
        /// Coverage already held.
        held: Sequence,
    },
}

/// The result of one attempt to fold a channel.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DigestOutcome {
    /// Nothing needed folding.
    Current,
    /// A new account, which the host may commit.
    Folded(ChannelDigest),
    /// No digester was supplied, or the one supplied failed.
    Unavailable,
    /// A digester answered and its answer was refused.
    Rejected {
        /// Precisely why.
        reason: DigestRejection,
    },
}

/// A history narrowed to what the account does not already cover.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DigestedHistory {
    /// The account to place before the rows, when there is one.
    pub digest: Option<String>,
    /// Inclusive sequence the account covers.
    pub covered_through: Option<Sequence>,
    /// Rows the account does not cover, chronological.
    pub messages: Vec<SessionMessage>,
}
