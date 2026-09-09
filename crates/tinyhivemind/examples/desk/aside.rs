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

use tinyhivemind::aside::{AsidePolicy, Audience};
use tinyhivemind::{LogMessage, SessionAuthor};

/// The aside policy this desk runs under.
///
/// A pair, six rows, and a settlement owed before another may be opened. A
/// desk solving one problem wants two seats to be able to sort out a
/// disagreement without spending the room's attention on it — and wants that
/// to end in something the room can read, which is what `must_surface` buys.
///
/// Read by `run.rs`, which hands it to the library's fold and to
/// [`TeamBriefing`](tinyhivemind::TeamBriefing), so a seat is told the rules
/// it is held to.
pub(crate) const ASIDES: AsidePolicy = AsidePolicy {
    enabled: true,
    max_members: 1,
    max_messages: 6,
    must_surface: true,
    require_thread: false,
};

/// How many rows the currently open aside has already spent.
///
/// Folded from the journal rather than stored, because this host keeps no
/// state the journal does not already carry. Every row in the run of
/// consecutive non-desk rows counts, not only the ones one speaker wrote: two
/// peers alternating inside the same aside share one budget.
pub(crate) fn spent_in_aside(rows: &[LogMessage]) -> usize {
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
pub(crate) fn unsettled_aside(rows: &[LogMessage], author_id: &str, peers: &[String]) -> bool {
    let mut participants = peers.to_vec();
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
