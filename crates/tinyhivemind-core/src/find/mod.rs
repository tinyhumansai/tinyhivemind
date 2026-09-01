//! Finding an agent, a person or a desk by name, over borrowed snapshots.
//!
//! These are the two pickers that need no host read: everything they search is
//! already in the roster and desk snapshots the caller holds for the turn.
//! Searching a *transcript* has to wait on a log, so it lives behind the
//! session port in the `tinyhivemind` crate — the same split the rest of this
//! workspace uses.
//!
//! Every function here is [`rank`](crate::select::rank) over candidates read off a
//! snapshot, so the ordering is one ordering, described once in
//! [`select`](crate::select).
//!
//! # Example
//!
//! ```
//! use tinyhivemind_core::{
//!     find,
//!     roster::{Roster, RosterMember},
//!     select::SELECT_LIMIT,
//! };
//!
//! let members = [
//!     RosterMember { id: "alice".into(), name: Some("Alice Nakamura".into()) },
//!     RosterMember { id: "bob".into(), name: Some("Bob Ferrante".into()) },
//! ];
//! let roster = Roster::new(&members, &[], &[]);
//! let hits = find::agents("naka", &roster, SELECT_LIMIT);
//! assert_eq!(hits[0].id, "alice");
//! ```

#[cfg(test)]
mod test;

use crate::{
    desk::DeskSet,
    roster::Roster,
    select::{Candidate, Hit, Pattern, rank_pattern},
};

/// Find active agents whose id or display name matches a query.
///
/// Retired agents are never offered: a picker exists to start something, and
/// nothing can be started with a retired agent.
#[must_use]
pub fn agents<'a>(query: &str, roster: &Roster<'a>, limit: usize) -> Vec<Hit<'a>> {
    agents_matching(&Pattern::Text(query), roster, limit)
}

/// Find active agents matching a pattern; see [`agents`].
#[must_use]
pub fn agents_matching<'a>(
    pattern: &Pattern<'_>,
    roster: &Roster<'a>,
    limit: usize,
) -> Vec<Hit<'a>> {
    let candidates: Vec<Candidate<'a>> = roster
        .active_members()
        .map(|member| {
            Candidate::new(
                member.id.as_str(),
                member.name.as_deref().unwrap_or(member.id.as_str()),
            )
        })
        .collect();
    rank_pattern(pattern, &candidates, limit)
}

/// Find people whose id or label matches a query.
#[must_use]
pub fn people<'a>(query: &str, roster: &Roster<'a>, limit: usize) -> Vec<Hit<'a>> {
    people_matching(&Pattern::Text(query), roster, limit)
}

/// Find people matching a pattern; see [`people`].
#[must_use]
pub fn people_matching<'a>(
    pattern: &Pattern<'_>,
    roster: &Roster<'a>,
    limit: usize,
) -> Vec<Hit<'a>> {
    let candidates: Vec<Candidate<'a>> = roster
        .people()
        .map(|person| Candidate::new(person.id.as_str(), person.label.as_str()))
        .collect();
    rank_pattern(pattern, &candidates, limit)
}

/// Find desks whose id, name or description matches a query.
///
/// A desk's description is supporting text and is scored at half weight, so a
/// desk named for the query always outranks one that merely mentions it. A
/// desk id declared twice is offered once, in its first declared position.
#[must_use]
pub fn desks<'a>(query: &str, desks: &DeskSet<'a>, limit: usize) -> Vec<Hit<'a>> {
    desks_matching(&Pattern::Text(query), desks, limit)
}

/// Find desks matching a pattern; see [`desks`].
#[must_use]
pub fn desks_matching<'a>(
    pattern: &Pattern<'_>,
    desks: &DeskSet<'a>,
    limit: usize,
) -> Vec<Hit<'a>> {
    let mut candidates: Vec<Candidate<'a>> = Vec::new();
    for desk in desks.iter() {
        if candidates
            .iter()
            .any(|candidate| candidate.id == desk.id.as_str())
        {
            continue;
        }
        let candidate = Candidate::new(desk.id.as_str(), desk.name.as_str());
        candidates.push(match desk.description.as_deref() {
            Some(description) => candidate.with_detail(description),
            None => candidate,
        });
    }
    rank_pattern(pattern, &candidates, limit)
}
