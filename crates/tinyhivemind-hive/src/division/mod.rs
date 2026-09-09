//! Division of labour: a task's facets, split across the seats that own them.
//!
//! Every other mechanism in this crate helps a room decide **one** question.
//! This one decides how many questions there are and who takes each, and it
//! exists because that turned out to be the thing worth having.
//!
//! # What was measured
//!
//! Two experiments, one after the other. `docs/experiments/2026-09-09-the-long-horizon.md`
//! asked whether a room earns its price on a task that gets *longer* and
//! answered no: a single agent that compacts by a superseding account beat a
//! room of five at every horizon and every window, at a seventh of the depth.
//!
//! `docs/experiments/2026-09-09-variety-and-roles.md` asked the other half —
//! whether it earns it on a task that gets *wider* — and answered yes, with a
//! crossover at **two facets**:
//!
//! ```text
//! all-facets %       F=1      F=2      F=4      F=8
//! one agent         78.7     56.9     28.1      7.8
//! one seat each     78.7     61.1     35.9     13.0
//! ```
//!
//! At one facet the two are identical and a room is the wrong tool. From two
//! on, a seat's per-facet accuracy is **flat in width** where the soloist's
//! decays, because a seat never holds the facets it is not deciding: 32 rows
//! against 160, and two rounds against eight.
//!
//! # The two halves, and why both are here
//!
//! A division is not only an assignment. The benchmark is specific about what
//! the win is made of, and dropping either half loses it:
//!
//! - **Who owns what.** Dividing a task across seats buys, on its own, *zero*.
//!   What buys is dividing it along a line where competence differs, which is
//!   why [`divide`] asks the [`Directory`] rather than only counting seats.
//! - **What each owner reads.** [`Division::scoped`] is the half that actually
//!   wins: an owner sees its own facet's rows and none of the others'. A host
//!   that assigns facets and then hands every seat the whole transcript has
//!   paid for the split and kept the load.
//!
//! There is a third property the benchmark measured and this module does not
//! provide, because it is the caller's: an owner must still see **its peers'
//! readings of its own facet**. Splitting a task and pooling nothing scored
//! 1.7 against the division's 13.0 — worse than not splitting at all. The
//! rows are in scope; putting them there is the host's job.
//!
//! # Purity
//!
//! [`divide`] is a fold over arguments the caller already holds, like every
//! other entry point here. It opens nothing, awaits nothing, stores nothing
//! between calls, and returns the same division for the same inputs. The
//! concurrency it authorizes is a *number of rounds*; the host does the
//! waiting.
//!
//! See [`README.md`][readme] in this directory for the design, and
//! `docs/specs/task-variety.md` for the behaviour and its known weaknesses.
//!
//! [readme]: https://github.com/tinyhumansai/tinyhivemind/blob/main/crates/tinyhivemind-hive/src/division/README.md
//! [`Directory`]: crate::Directory

#[cfg(test)]
mod test;

mod types;

pub use types::{Assignment, Division, DivisionPolicy, OwnerReason};

use tinyhivemind::{desk::DeskSet, roster::Roster};

use crate::directory::Directory;
use crate::error::Error;
use crate::trace::TopicId;
use crate::{Result, error};

/// Divide a task's facets across a desk's active members.
///
/// The facets are the caller's: this crate does not decide what a task is made
/// of, only who takes each piece of one and in what order. Facets are answered
/// in the order given, deduplicated — a facet named twice is one facet — and
/// packed into the fewest rounds that respect two bounds at once: no round
/// runs more than [`DivisionPolicy::round_width`] assignments, and no seat
/// appears twice in a round, because one agent cannot answer two questions
/// concurrently.
///
/// A task of one facet divides into one assignment in one round, which is a
/// single agent answering alone. That is not a special case in the code and
/// must not become one: it is the measured rule falling out of the general
/// one, and [`Division::is_alone`] is how a caller reads it.
///
/// `known` is optional because a room that has not deliberated yet has no
/// directory to fold. Passing `None`, or a directory with no opinion on a
/// facet, falls to the rotation — deterministically, and recorded as
/// [`OwnerReason::Rotation`] so a caller can tell an informed assignment from
/// an arbitrary one.
///
/// # Errors
///
/// Returns [`Error::ZeroRoundWidth`] when the policy authorizes no width at
/// all, [`Error::NoSeats`] when the desk has no active member to own a facet,
/// and [`Error::Core`] when the roster or desk snapshot is structurally
/// invalid or the desk is unknown.
pub fn divide(
    facets: &[TopicId],
    desk_id: &str,
    roster: &Roster<'_>,
    desks: &DeskSet<'_>,
    known: Option<&Directory>,
    policy: &DivisionPolicy,
) -> Result<Division> {
    roster.validate()?;
    desks.validate()?;
    if policy.round_width == 0 {
        return Err(Error::ZeroRoundWidth);
    }
    let desk_id = desks.resolve_id(desk_id)?;
    let seats: Vec<&str> = desks
        .members(desk_id)?
        .into_iter()
        .filter(|id| roster.active_member(id).is_some())
        .collect();
    if seats.is_empty() {
        return Err(Error::NoSeats {
            desk_id: desk_id.to_owned(),
        });
    }
    let facets = distinct(facets);
    if facets.is_empty() {
        return Ok(Division::default());
    }

    let owned: Vec<(TopicId, String, OwnerReason)> = facets
        .into_iter()
        .enumerate()
        .map(|(index, facet)| {
            let (owner, reason) = owner_of(&facet, index, &seats, known, policy);
            (facet, owner, reason)
        })
        .collect();

    Ok(Division {
        assignments: packed(owned, seats.len(), policy.round_width),
    })
}

/// The facets, deduplicated, in the order first given.
fn distinct(facets: &[TopicId]) -> Vec<TopicId> {
    let mut seen: Vec<TopicId> = Vec::with_capacity(facets.len());
    for facet in facets {
        if !seen.contains(facet) {
            seen.push(facet.clone());
        }
    }
    seen
}

/// Who takes one facet: whoever the directory names on it, else the rotation.
///
/// The rotation is by the facet's own index rather than by a running counter,
/// so which seat takes a facet does not depend on how many facets before it
/// happened to have a known owner. A division is a fold, and a fold whose
/// answer for one element depended on the path taken to it would not be one.
fn owner_of(
    facet: &TopicId,
    index: usize,
    seats: &[&str],
    known: Option<&Directory>,
    policy: &DivisionPolicy,
) -> (String, OwnerReason) {
    if policy.follow_directory
        && let Some(expert) = known.and_then(|known| known.top_among(facet, seats))
    {
        return (expert.to_owned(), OwnerReason::Knows);
    }
    let seat = seats.get(index % seats.len()).copied().unwrap_or_default();
    (seat.to_owned(), OwnerReason::Rotation)
}

/// Pack owned facets into the fewest rounds two bounds allow.
///
/// Greedy and stable: each assignment takes the lowest round that is not full
/// and does not already hold its owner. Greedy is optimal here because both
/// constraints are per-round capacities rather than orderings — an assignment
/// placed as early as it can go never forces a later one further out than it
/// would otherwise have gone.
fn packed(
    owned: Vec<(TopicId, String, OwnerReason)>,
    seats: usize,
    round_width: u32,
) -> Vec<Assignment> {
    let lane = usize::try_from(round_width).unwrap_or(usize::MAX).min(seats);
    let mut rounds: Vec<Vec<String>> = Vec::new();
    let mut assignments = Vec::with_capacity(owned.len());
    for (facet, owner, reason) in owned {
        let mut round = 0_usize;
        loop {
            match rounds.get(round) {
                Some(held) if held.len() >= lane.max(1) || held.contains(&owner) => {
                    round = round.saturating_add(1);
                }
                Some(_) => break,
                None => {
                    rounds.push(Vec::new());
                    break;
                }
            }
        }
        if let Some(held) = rounds.get_mut(round) {
            held.push(owner.clone());
        }
        assignments.push(Assignment {
            facet,
            owner,
            round: u32::try_from(round).unwrap_or(u32::MAX),
            reason,
        });
    }
    assignments
}

// Re-exported for the doc link above, which names the module rather than the
// type so a reader lands on the error list rather than on one variant.
use error as _error_docs;
