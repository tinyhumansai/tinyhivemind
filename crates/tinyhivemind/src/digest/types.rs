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
    /// Characters of foldable content that trigger a fold on their own.
    ///
    /// Twenty rows of acknowledgements and twenty rows of derivations are the
    /// same row count and very different costs, and what a host actually
    /// budgets is how much of a seat's context window the room's scrollback
    /// may occupy. Whichever threshold binds first wins; `0` disables this one.
    ///
    /// The unit is characters rather than tokens on purpose — see
    /// [`Self::from_token_budget`].
    ///
    /// Defaults to [`DigestPolicy::DEFAULT`]'s safety ceiling when absent from
    /// the wire, so a policy a host stored before this field existed still
    /// decodes — into the same generous ceiling a host that never sets it
    /// gets today, not a missing-field error.
    #[serde(default = "default_fold_after_chars")]
    pub fold_after_chars: usize,
    /// Most rows handed to one digester call.
    pub input_limit: usize,
    /// Most characters a digest may occupy.
    pub budget_chars: usize,
}

/// [`DigestPolicy::fold_after_chars`]'s serde default: the same ceiling
/// [`DigestPolicy::DEFAULT`] carries, so an older stored payload decodes into
/// exactly what a host that never set the field gets today.
const fn default_fold_after_chars() -> usize {
    DigestPolicy::DEFAULT.fold_after_chars
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
        // A ceiling rather than a target: at four characters to the token this
        // is roughly 100k tokens of scrollback, which no existing caller
        // reaches on rows alone, so a host that never sets it gains a safety
        // net and not a change of shape.
        fold_after_chars: 400_000,
        input_limit: 60,
        budget_chars: 4000,
    };

    /// Characters assumed per token when a host states its budget in tokens.
    ///
    /// A proxy, and stated as one. English prose and code run roughly 3.5–4.5
    /// characters to the token, so a budget converted through this is within
    /// about 15% of its intent — the right precision for deciding *when to
    /// spend one summarization call*, and the wrong tool for anything that has
    /// to be exact.
    ///
    /// The alternative is a tokenizer, which this crate may not take: it is a
    /// per-model table, so the same channel would fold at different points on
    /// two machines and the plan would stop being a fold over its arguments.
    pub const CHARS_PER_TOKEN: usize = 4;

    /// The default policy, folding once the scrollback would cost `tokens`.
    ///
    /// A host writes its budget in the unit it thinks in and the conversion
    /// happens here, once. Saturates rather than overflowing.
    #[must_use]
    pub const fn from_token_budget(tokens: usize) -> Self {
        Self {
            fold_after_chars: tokens.saturating_mul(Self::CHARS_PER_TOKEN),
            ..Self::DEFAULT
        }
    }
}

/// Where a channel currently stands, as the planner needs to see it.
///
/// Two numbers, because there are two thresholds. The character count is the
/// host's to keep: it appends the rows, so it is the only party that sees
/// their size without reading the log back, and reading the log back on every
/// turn to decide whether to compact it would cost more than the compaction
/// saves.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChannelHead {
    /// The newest sequence the channel holds.
    pub sequence: Sequence,
    /// Characters of desk-visible content the account does not yet cover.
    ///
    /// Rows in the live tail count: they are foldable in principle and will be
    /// folded as the room moves past them. Private rows do not — an account
    /// may not contain one, so one must not be able to trigger a fold either.
    pub unfolded_chars: usize,
}

impl ChannelHead {
    /// A head with no character count, planning on rows alone.
    #[must_use]
    pub const fn at(sequence: Sequence) -> Self {
        Self {
            sequence,
            unfolded_chars: 0,
        }
    }
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
    /// Sequences of pinned messages at or below `through`, ascending.
    ///
    /// A pin is the room saying *this one does not scroll away*, and a fold
    /// that drops it has undone the pin without anybody deciding to. The
    /// digester is told which of the rows it holds carry that claim so it can
    /// keep what they say; nothing here carries content, so a pin to a private
    /// row could not leak one even if a host offered it.
    pub pinned: Vec<Sequence>,
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
