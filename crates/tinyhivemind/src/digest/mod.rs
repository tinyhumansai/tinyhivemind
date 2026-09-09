//! Bounded, superseding compaction of a shared channel.
//!
//! A desk's transcript outgrows every window it is read through. The library
//! already answers that for one turn by projecting a bounded slice, but a
//! slice is not an account: a seat that joins on turn forty is handed the last
//! thirty rows and no idea what the first four hundred established.
//!
//! This module adds the other half. A channel carries one **account** — a
//! bounded text standing for every row older than the live tail — which is
//! rewritten as the room moves and always covers strictly more than it did
//! before. A turn is then composed of the account plus the rows the account
//! does not cover, so what a reader spends its window on is the live
//! conversation rather than its own scrollback.
//!
//! Three rules keep this from becoming a second journal:
//!
//! 1. **The account is not a row.** It supersedes itself, and an append-only
//!    log that never condenses cannot hold a superseding record without either
//!    growing without bound or being asked to forget. It is host state, of
//!    exactly the kind [`SharingState`](crate::SharingState) already is.
//! 2. **The account folds only what every member may read.** Rows outside
//!    [`Audience::Desk`] are skipped, so one account serves every member of
//!    the channel — including one that has never spoken — and no fold can
//!    launder a private row into a shared summary.
//! 3. **The rows survive.** Folding changes what a turn is *shown*, never what
//!    the log holds. Every folded row keeps its sequence and stays reachable
//!    through [`search_messages`](crate::search_messages).
//!
//! The fold itself needs a model, so it is a port — [`Digester`] — in the same
//! shape as [`Selector`](crate::Selector): a request in, text out, no host
//! handle and no callback. Everything around it is a pure fold, and a digester
//! that is missing, slow, or wrong costs the room its compaction and nothing
//! else.
//!
//! # Example
//!
//! ```
//! use tinyhivemind::{
//!     ChannelDigest, Conversation, DigestPlan, DigestPolicy, Sequence, plan_digest,
//! };
//!
//! let policy = DigestPolicy::DEFAULT;
//! let conversation = Conversation {
//!     desk_id: "engineering".into(),
//!     desk_name: "Engineering".into(),
//!     thread_root: None,
//! };
//!
//! // A short room is left alone.
//! assert_eq!(plan_digest(None, Sequence(12), policy), DigestPlan::Current);
//!
//! // A long one folds everything behind the live tail, in bounded steps.
//! assert_eq!(
//!     plan_digest(None, Sequence(400), policy),
//!     DigestPlan::Fold { after: None, through: Sequence(60) },
//! );
//!
//! // And the next step starts where the account already reaches.
//! let held = ChannelDigest {
//!     conversation,
//!     through: Sequence(60),
//!     covered: 60,
//!     generation: 1,
//!     text: "the room chose the sublinear rank".into(),
//! };
//! assert_eq!(
//!     plan_digest(Some(&held), Sequence(400), policy),
//!     DigestPlan::Fold { after: Some(Sequence(60)), through: Sequence(120) },
//! );
//! ```

#[cfg(test)]
mod test;

mod types;

pub use types::{
    ChannelDigest, ChannelHead, DigestOutcome, DigestPlan, DigestPolicy, DigestRejection,
    DigestRequest,
    DigestedHistory,
};

use crate::{
    Conversation, Error, PAGE_SIZE, Result, SCAN_LIMIT, Sequence, SessionLog, SessionMessage,
    responder::BoxError,
    session::{matches_conversation, validate_page},
};
use std::{future::Future, pin::Pin};

/// The boxed, executor-neutral future returned by [`Digester`].
pub type DigestFuture<'a> =
    Pin<Box<dyn Future<Output = std::result::Result<String, BoxError>> + Send + 'a>>;

/// A model-backed folder with no transcript access, tools, or host handles.
///
/// The trait is object-safe and receives only the bounded rows to fold and the
/// account being superseded.
pub trait Digester: Send + Sync {
    /// Return text intended to stand for `request.messages`.
    fn digest<'a>(&'a self, request: &'a DigestRequest) -> DigestFuture<'a>;
}

/// Decide what the next fold of a channel should cover.
///
/// `head` is the newest sequence in the log. The trigger is measured in
/// sequence distance rather than in rows of this channel, which overestimates
/// when a log carries several channels: a busy neighbour makes this fold
/// *earlier* than strictly necessary, never later, and folding early costs one
/// call rather than correctness.
///
/// The upper bound advances by at most [`DigestPolicy::input_limit`] per call,
/// so a first fold over a long history is a run of bounded calls rather than
/// one enormous one, and no row is ever stepped over.
#[must_use]
pub fn plan_digest(
    account: Option<&ChannelDigest>,
    head: ChannelHead,
    policy: DigestPolicy,
) -> DigestPlan {
    let folded = account.map_or(0, |digest| digest.through.0);
    let Some(ceiling) = head.sequence.0.checked_sub(policy.keep_live as u64) else {
        return DigestPlan::Current;
    };
    if ceiling <= folded {
        return DigestPlan::Current;
    }
    // Either threshold is enough. Rows say the room has *moved*; characters
    // say it has grown expensive, and on a desk writing derivations the second
    // arrives long before the first.
    let by_rows = ceiling - folded > policy.fold_after as u64;
    let by_size = policy.fold_after_chars > 0 && head.unfolded_chars > policy.fold_after_chars;
    if !by_rows && !by_size {
        return DigestPlan::Current;
    }
    let step = folded.saturating_add(policy.input_limit as u64);
    DigestPlan::Fold {
        after: account.map(|digest| digest.through),
        through: Sequence(ceiling.min(step)),
    }
}

/// Read the desk-visible rows one fold step covers, chronologically.
///
/// # Errors
///
/// Returns [`Error::Read`] for a host read failure, a page-validation error
/// for a malformed page, or [`Error::DigestGap`] when the bounded scan ends
/// before reaching `after` — a host log that cannot produce the range cannot
/// be folded over it, and a fold with a hole in it is worse than none.
pub async fn collect_digest_input(
    log: &(dyn SessionLog + '_),
    conversation: &Conversation,
    after: Option<Sequence>,
    through: Sequence,
) -> Result<Vec<SessionMessage>> {
    let floor = after.map_or(0, |sequence| sequence.0);
    let mut cursor = Some(Sequence(through.0.saturating_add(1)));
    let mut scanned = 0_usize;
    let mut seen = Vec::new();
    let mut collected: Vec<SessionMessage> = Vec::new();
    let mut reached = false;

    while scanned < SCAN_LIMIT && !reached {
        let limit = PAGE_SIZE.min(SCAN_LIMIT - scanned);
        let page = log
            .read_before(cursor, limit)
            .await
            .map_err(|source| Error::Read { source })?;
        validate_page(&page, cursor, limit, &mut seen)?;
        for raw in &page.messages {
            scanned += 1;
            if raw.sequence.0 <= floor {
                reached = true;
                break;
            }
            if matches_conversation(raw, conversation)
                && raw.audience.is_desk()
                && !raw.content.trim().is_empty()
            {
                collected.push(SessionMessage {
                    sequence: raw.sequence,
                    author: raw.author.clone(),
                    content: raw.content.clone(),
                    audience: raw.audience.clone(),
                    elided: None,
                });
            }
        }
        match page.next_before {
            // The log ended. Everything above the floor that exists has been
            // collected, which is the whole range even when rows below it are
            // gone: a fold is over what the channel holds, not what it once did.
            None => reached = true,
            Some(next) => cursor = Some(next),
        }
    }

    if !reached {
        return Err(Error::DigestGap {
            after: Sequence(floor),
            scanned,
        });
    }
    collected.reverse();
    Ok(collected)
}

/// Validate a digester's answer against the account it replaces.
///
/// # Errors
///
/// Returns a [`DigestRejection`] — not an [`Error`] — when the answer is
/// empty, overruns its budget, or would cover no more than the account it
/// replaces. It is not a crate error because nothing is lost by it: the held
/// account and the live tail both still stand.
pub fn accept_digest(
    account: Option<&ChannelDigest>,
    request: &DigestRequest,
    text: &str,
) -> std::result::Result<ChannelDigest, DigestRejection> {
    let text = text.trim();
    if text.is_empty() {
        return Err(DigestRejection::Empty);
    }
    let actual = text.chars().count();
    if actual > request.budget_chars {
        return Err(DigestRejection::TooLarge {
            limit: request.budget_chars,
            actual,
        });
    }
    if let Some(account) = account
        && request.through <= account.through
    {
        return Err(DigestRejection::Regressed {
            through: request.through,
            held: account.through,
        });
    }
    Ok(ChannelDigest {
        conversation: request.conversation.clone(),
        through: request.through,
        covered: account.map_or(0, |digest| digest.covered) + request.messages.len() as u64,
        generation: account.map_or(0, |digest| digest.generation) + 1,
        text: text.to_string(),
    })
}

/// Fold one step of a channel, invoking the digester at most once.
///
/// A missing or failing digester yields [`DigestOutcome::Unavailable`] rather
/// than an error, exactly as an unavailable
/// [`Selector`](crate::Selector) yields a fallback: compaction is an
/// optimization over a projection that is already correct without it.
///
/// # Errors
///
/// Returns [`Error::DigestConversationChanged`] when the held account belongs
/// to another channel, and the read failures of [`collect_digest_input`].
pub async fn refold(
    log: &(dyn SessionLog + '_),
    digester: Option<&(dyn Digester + '_)>,
    conversation: &Conversation,
    account: Option<&ChannelDigest>,
    head: ChannelHead,
    pins: &[Pin],
    policy: DigestPolicy,
) -> Result<DigestOutcome> {
    if let Some(account) = account
        && !account.conversation.equivalent_to(conversation)
    {
        return Err(Error::DigestConversationChanged {
            held: account.conversation.desk_id.clone(),
            requested: conversation.desk_id.clone(),
        });
    }
    let DigestPlan::Fold { after, through } = plan_digest(account, head, policy) else {
        return Ok(DigestOutcome::Current);
    };
    let Some(digester) = digester else {
        return Ok(DigestOutcome::Unavailable);
    };
    let messages = collect_digest_input(log, conversation, after, through).await?;
    if messages.is_empty() {
        // Every row in the step was private, empty, or another channel's.
        // There is nothing to say about it, but the account must still advance
        // or the same empty step is planned forever.
        return Ok(match account {
            Some(account) => DigestOutcome::Folded(ChannelDigest {
                through,
                ..account.clone()
            }),
            None => DigestOutcome::Current,
        });
    }
    let request = DigestRequest {
        conversation: conversation.clone(),
        prior: account.map(|digest| digest.text.clone()),
        messages,
        through,
        budget_chars: policy.budget_chars,
        pinned: pinned_through(pins, through),
    };
    let Ok(text) = digester.digest(&request).await else {
        return Ok(DigestOutcome::Unavailable);
    };
    Ok(match accept_digest(account, &request, &text) {
        Ok(digest) => DigestOutcome::Folded(digest),
        Err(reason) => DigestOutcome::Rejected { reason },
    })
}

/// The pinned sequences a fold reaching `through` is answerable for.
///
/// A pin above `through` is still in the live tail, where the reader sees the
/// row itself; the fold has no business with it. Ascending, deduplicated, so
/// two markers pinning one row name it once.
fn pinned_through(pins: &[Pin], through: Sequence) -> Vec<Sequence> {
    let mut sequences: Vec<Sequence> = pins
        .iter()
        .map(|pin| pin.sequence)
        .filter(|sequence| sequence.0 <= through.0)
        .collect();
    sequences.sort_unstable_by_key(|sequence| sequence.0);
    sequences.dedup();
    sequences
}

/// Compose one turn's history from an account and a projected window.
///
/// Rows the account already covers are dropped: they are the ones the reader
/// would otherwise be shown twice, once as a summary and once in full, and a
/// window spent on both is a window spent on neither.
#[must_use]
pub fn apply_digest(
    account: Option<&ChannelDigest>,
    messages: &[SessionMessage],
) -> DigestedHistory {
    let Some(account) = account else {
        return DigestedHistory {
            digest: None,
            covered_through: None,
            messages: messages.to_vec(),
        };
    };
    DigestedHistory {
        digest: Some(account.text.clone()),
        covered_through: Some(account.through),
        messages: messages
            .iter()
            .filter(|message| message.sequence > account.through)
            .cloned()
            .collect(),
    }
}
