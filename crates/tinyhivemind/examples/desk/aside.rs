//! Private asides: this desk's policy, and the host-side bookkeeping
//! `tinyhivemind_core::aside::aside` needs to enforce it.
//!
//! The harness never acts on the `!aside` marker itself — it hands the line
//! to the library's `aside` fold, which resolves who it may reach and
//! refuses with a named reason otherwise. What lives here is only the
//! bookkeeping the fold cannot derive on its own: how much of the budget an
//! open aside has already spent, and whether a *different* prior aside among
//! the same participants is still unsettled. Both are folded out of the
//! transcript rather than stored, because this host keeps no state the
//! journal does not already carry.

use tinyhivemind::aside::{AsideDecision, AsideInput, AsidePolicy, Audience, aside};
use tinyhivemind::dispatch::DispatchConversation;
use tinyhivemind::mention::{Mention, MentionTarget};
use tinyhivemind::{LogMessage, SessionAuthor};

use crate::{BoxError, log};

/// The aside policy this desk runs under.
///
/// A pair, six rows, and a settlement owed before another may be opened. A
/// desk solving one problem wants two seats to be able to sort out a
/// disagreement without spending the room's attention on it — and wants that
/// to end in something the room can read, which is what `must_surface` buys.
///
/// Read by [`address`] below, and by `run.rs` to hand the same policy to
/// [`TeamBriefing`](tinyhivemind::TeamBriefing) so a seat is told the rules it
/// is held to.
pub(crate) const ASIDES: AsidePolicy = AsidePolicy {
    enabled: true,
    max_members: 1,
    max_messages: 6,
    must_surface: true,
    require_thread: false,
};

/// One authored line and what it is trying to reach.
#[derive(Clone, Copy)]
pub(crate) struct Addressed<'a> {
    /// The message itself.
    pub(crate) line: &'a str,
    /// The targets it names, or the ones `desk_dm` named for it.
    pub(crate) mentions: &'a [Mention],
    /// Whether the seat asked for this to stay off the desk.
    ///
    /// A `!aside` marker says so in the prose and is read out of `line`; a
    /// `desk_dm` call says so in a field, and this is that field. Either way
    /// the decision is still the library's.
    pub(crate) private: bool,
}

/// Decide who one authored line is addressed to.
///
/// The harness never acts on the `!aside` marker itself. It hands the line to
/// [`aside`], which resolves who it may reach and refuses with a named reason
/// otherwise; a refusal leaves the row desk-visible, which is the safe
/// direction to fail in.
pub(crate) fn address(
    transcript: &log::JsonlLog,
    desk_id: &str,
    author_id: &str,
    addressed: &Addressed<'_>,
    roster: &tinyhivemind::roster::Roster<'_>,
    desks: &tinyhivemind::desk::DeskSet<'_>,
) -> Result<Audience, BoxError> {
    let Addressed {
        line,
        mentions,
        private,
    } = *addressed;
    if !private && !line.trim_start().starts_with("!aside") {
        return Ok(Audience::Desk);
    }
    let rows = transcript.rows();
    let decision = aside(
        ASIDES,
        &AsideInput {
            conversation: DispatchConversation {
                desk_id: desk_id.to_string(),
                thread_root: None,
            },
            author_id: author_id.to_string(),
            mentions: mentions.to_vec(),
            spent: spent_in_aside(&rows),
            unsettled: unsettled_aside(&rows, author_id, mentions),
        },
        roster,
        desks,
    )?;
    Ok(match decision {
        AsideDecision::One { audience } => audience,
        AsideDecision::None { reason } => {
            println!("   aside refused: {reason:?} — the row stays desk-visible");
            Audience::Desk
        }
    })
}

/// How many rows the currently open aside has already spent.
///
/// Folded from the journal rather than stored, because this host keeps no
/// state the journal does not already carry. Every row in the run of
/// consecutive non-desk rows counts, not only the ones one speaker wrote: two
/// peers alternating inside the same aside share one budget.
fn spent_in_aside(rows: &[LogMessage]) -> usize {
    rows.iter()
        .rev()
        .take_while(|row| !row.audience.is_desk())
        .count()
}

/// Whether a *different* prior aside among the same participants is unsettled.
///
/// Replying inside the aside already in progress is not opening another one —
/// the budget above is what ends that run — so a line whose immediately
/// preceding row already carries this participant set is a continuation.
/// Walking the journal, an aside opens when its participant set matches and
/// closes the first time one of those participants speaks on the desk.
fn unsettled_aside(rows: &[LogMessage], author_id: &str, mentions: &[Mention]) -> bool {
    let mut participants = addressed(mentions, author_id);
    if participants.is_empty() {
        return false;
    }
    participants.push(author_id.to_string());
    participants.sort();
    participants.dedup();

    if let Some(row) = rows.last()
        && let Audience::Aside { members } = &row.audience
        && let SessionAuthor::Agent { id, .. } = &row.author
    {
        let mut here: Vec<String> = members.clone();
        here.push(id.clone());
        here.sort();
        here.dedup();
        if here == participants {
            return false;
        }
    }

    let mut open = false;
    for row in rows {
        let SessionAuthor::Agent { id, .. } = &row.author else {
            continue;
        };
        match &row.audience {
            Audience::Aside { members } => {
                let mut here: Vec<String> = members.clone();
                here.push(id.clone());
                here.sort();
                here.dedup();
                if here == participants {
                    open = true;
                }
            }
            Audience::Desk => {
                if participants.iter().any(|who| who == id) {
                    open = false;
                }
            }
        }
    }
    open
}

/// The active agent ids a line addresses, without the author and without
/// repeats, in the order they were written.
pub(crate) fn addressed(mentions: &[Mention], author_id: &str) -> Vec<String> {
    let mut ordered: Vec<&Mention> = mentions.iter().collect();
    ordered.sort_by_key(|mention| mention.offset);
    let mut out: Vec<String> = Vec::new();
    for mention in ordered {
        if mention.quiet {
            continue;
        }
        if let MentionTarget::Agent { id } = &mention.target
            && id != author_id
            && !out.contains(id)
        {
            out.push(id.clone());
        }
    }
    out
}
