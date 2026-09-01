//! Quorum as a local decaying count, and cross-inhibition that silences an
//! advocate rather than debiting an option.
//!
//! Two properties here are load-bearing, and both come from how honeybee
//! swarms actually settle on a nest site rather than from voting theory.
//!
//! **Quorum is local.** A topic carries when `threshold` *distinct*
//! participants have supported it within the last `window` sequences — not
//! when it holds a majority of anything. The count is order-independent and
//! idempotent, so a participant that catches up late folds to the same
//! standing as one that watched live.
//!
//! **Cross-inhibition targets the advocate, not the option.** An objection
//! naming a message removes that message's author from the supporter set of
//! the topic they were advocating. Subtracting from a score cannot break a tie
//! between two equally supported options; silencing an advocate can, and that
//! asymmetry is the entire reason the mechanism is shaped this way.
//!
//! **Refutation targets the option, not the advocate.** That is the other half
//! of the same model, and the library shipped without it. In the bee model this
//! crate borrows from, the stop signal is one term and a scout's own assessment
//! of the site's *value* is another; evidence bearing on a site lowers what
//! every scout would independently conclude about it, rather than silencing any
//! one dancer. `!refute #topic ^N` is that term. It caps rather than debits,
//! because `carried` reads a supporter count and a debit against the weight
//! would change nothing. See
//! `docs/adr/0003-refutation-links-evidence-to-a-topic.md`.
//!
//! **Grounds are weighed, not counted.** A support citing another support is a
//! citation of an opinion, which is exactly the condition under which an
//! information cascade forms. Under `require_evidential` a support counts only
//! if its citation chain reaches a stated fact. See
//! `docs/adr/0004-grounds-are-weighed-by-evidential-depth.md`.

#[cfg(test)]
mod test;

mod types;

pub use types::{ConsensusState, QuorumPolicy, TopicStanding};

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    error::{Error, Result},
    salience::importance,
    trace::{TopicId, Trace, TraceKind},
};
use tinyhivemind::Sequence;

/// Fold traces into one standing per topic.
///
/// Only [`TraceKind::Propose`] and [`TraceKind::Support`] add a supporter, and
/// only within `policy.window` sequences of `at`. Under
/// `policy.require_grounded`, support that cites nothing is ignored entirely:
/// it joins neither the supporter set nor the weight.
///
/// Under `policy.require_evidential`, which implies `require_grounded`, support
/// counts only when its citation chain reaches a [`TraceKind::Evidence`], and
/// an objection silences nobody unless its own author deposited evidence in the
/// window.
///
/// A [`TraceKind::Refute`] naming a topic adds its author to that topic's
/// `refuted_by`. It attaches only to a topic some member actually advocated:
/// refuting something nobody put on the floor is inert, so one member cannot
/// manufacture a standing. A member that both supports and refutes the same
/// topic in window is counted as a refuter only — the more specific move is the
/// one it meant.
///
/// Objections and refutations are applied after all support, so the result does
/// not depend on the order traces arrived in.
///
/// # Errors
///
/// Returns [`Error::ZeroQuorumThreshold`], [`Error::ZeroQuorumWindow`], or
/// [`Error::ZeroRefutationCap`] when the policy would make the count
/// meaningless.
pub fn standings(
    traces: &[Trace],
    at: Sequence,
    policy: &QuorumPolicy,
) -> Result<Vec<TopicStanding>> {
    if policy.threshold == 0 {
        return Err(Error::ZeroQuorumThreshold);
    }
    if policy.window == 0 {
        return Err(Error::ZeroQuorumWindow);
    }
    if policy.refutation_cap == Some(0) {
        return Err(Error::ZeroRefutationCap);
    }

    let floor = at.0.saturating_sub(u64::from(policy.window));
    let mut live: Vec<&Trace> = traces
        .iter()
        .filter(|trace| trace.sequence.0 >= floor && trace.sequence <= at)
        .collect();
    // A trace is addressed by where it was authored, so `(sequence, offset)`
    // identifies it. Sorting and deduplicating on that address is what makes
    // this fold commutative and idempotent: a redelivered trace, or a caller
    // that folds an unordered list, lands in exactly the same place as one
    // that saw the medium in order.
    live.sort_by_key(|trace| (trace.sequence, trace.offset));
    live.dedup_by_key(|trace| (trace.sequence, trace.offset));

    // `require_evidential` is the stronger claim and subsumes the weaker one:
    // an uncited support has no chain to resolve, so requiring the chain to
    // reach a fact already requires a chain.
    let require_grounded = policy.require_grounded || policy.require_evidential;

    let by_sequence = index_by_sequence(&live);
    let evidenced = evidenced_authors(&live);
    let refuters = refuter_pairs(&live);

    // Every `(agent, topic)` a message advocated, keyed by that message's
    // sequence. One message can carry several propose/support traces at
    // different offsets -- one per topic -- so an objection naming that
    // message must be able to silence the advocate on *every* topic it
    // advocated there, not just the last one folded.
    //
    // Every map here is keyed by a borrow of the traces being folded rather
    // than by an owned copy. The fold runs on every step of every episode, and
    // the owned `TopicStanding` is built once at the end from what survives.
    let mut advocacy: BTreeMap<Sequence, Vec<(&str, &TopicId)>> = BTreeMap::new();
    let mut ordered: Vec<&TopicId> = Vec::new();
    let mut supporters: BTreeMap<&TopicId, Vec<&str>> = BTreeMap::new();
    // Weight per topic, per contributing agent, so silencing one advocate can
    // remove exactly their contribution rather than either leaving the whole
    // sum untouched or zeroing every other supporter's weight along with it.
    let mut weight: BTreeMap<&TopicId, BTreeMap<&str, i64>> = BTreeMap::new();

    for trace in &live {
        if !matches!(trace.kind, TraceKind::Propose | TraceKind::Support) {
            continue;
        }
        let (Some(topic), Some(agent)) = (trace.topic.as_ref(), trace.agent_id()) else {
            continue;
        };
        if require_grounded && trace.kind == TraceKind::Support && !trace.grounded() {
            continue;
        }
        if policy.require_evidential
            && trace.kind == TraceKind::Support
            && !reaches_evidence(trace, &by_sequence)
        {
            continue;
        }
        if refuters.contains(&(agent, topic)) {
            continue;
        }
        if !ordered.contains(&topic) {
            ordered.push(topic);
        }
        advocacy
            .entry(trace.sequence)
            .or_default()
            .push((agent, topic));
        let entry = supporters.entry(topic).or_default();
        if !entry.contains(&agent) {
            entry.push(agent);
        }
        *weight.entry(topic).or_default().entry(agent).or_default() += importance(trace.kind);
    }

    let mut silenced = silenced_advocates(
        &live,
        &advocacy,
        &Gate {
            require_grounded,
            require_evidential: policy.require_evidential,
            evidenced: &evidenced,
        },
    );

    let mut refuted = refutations(&live, &ordered);

    Ok(ordered
        .into_iter()
        .map(|topic| {
            let silenced = silenced.remove(&topic).unwrap_or_default();
            let refuted_by: Vec<String> = refuted
                .remove(&topic)
                .unwrap_or_default()
                .into_iter()
                .map(str::to_owned)
                .collect();
            let supporters: Vec<String> = supporters
                .remove(&topic)
                .unwrap_or_default()
                .into_iter()
                .filter(|agent| !silenced.contains(agent))
                .map(str::to_owned)
                .collect();
            let support: i64 = weight
                .remove(&topic)
                .unwrap_or_default()
                .into_iter()
                .filter(|(agent, _)| !silenced.contains(agent))
                .map(|(_, contribution)| contribution)
                .sum();
            TopicStanding {
                topic: topic.clone(),
                supporters,
                silenced: silenced.into_iter().map(str::to_owned).collect(),
                refuted_by,
                support,
            }
        })
        .collect())
}

/// What a negative move must satisfy before it counts.
struct Gate<'a> {
    /// Whether it must cite anything at all.
    require_grounded: bool,
    /// Whether its author must also have put a fact on the floor.
    require_evidential: bool,
    /// The authors who have.
    evidenced: &'a BTreeSet<&'a str>,
}

/// Apply cross-inhibition: which advocates an objection removes, per topic.
///
/// An objection cannot silence its own author. That would let an agent retract
/// another's support by objecting to itself.
fn silenced_advocates<'a>(
    live: &[&'a Trace],
    advocacy: &BTreeMap<Sequence, Vec<(&'a str, &'a TopicId)>>,
    gate: &Gate<'_>,
) -> BTreeMap<&'a TopicId, Vec<&'a str>> {
    let mut silenced: BTreeMap<&'a TopicId, Vec<&'a str>> = BTreeMap::new();
    for trace in live {
        if trace.kind != TraceKind::Object {
            continue;
        }
        if gate.require_grounded && !trace.grounded() {
            continue;
        }
        if gate.require_evidential
            && !trace
                .agent_id()
                .is_some_and(|agent| gate.evidenced.contains(agent))
        {
            continue;
        }
        let Some(target) = trace.target else { continue };
        let Some(advocacies) = advocacy.get(&target) else {
            continue;
        };
        for (advocate, topic) in advocacies {
            if trace.agent_id() == Some(*advocate) {
                continue;
            }
            let entry = silenced.entry(topic).or_default();
            if !entry.contains(advocate) {
                entry.push(advocate);
            }
        }
    }
    silenced
}

/// Index every live trace by the sequence a citation would name.
///
/// One message can carry several traces at different offsets, so a citation
/// resolves to a list rather than to a single trace.
fn index_by_sequence<'a>(live: &[&'a Trace]) -> BTreeMap<Sequence, Vec<&'a Trace>> {
    let mut by_sequence: BTreeMap<Sequence, Vec<&'a Trace>> = BTreeMap::new();
    for trace in live {
        by_sequence.entry(trace.sequence).or_default().push(trace);
    }
    by_sequence
}

/// Which members have put a fact on the floor at all.
///
/// Under `require_evidential` this gates objecting as well as supporting: the
/// bee stop signal is delivered by a scout who inspected the rival site.
fn evidenced_authors<'a>(live: &[&'a Trace]) -> BTreeSet<&'a str> {
    live.iter()
        .filter(|trace| trace.kind == TraceKind::Evidence)
        .filter_map(|trace| trace.agent_id())
        .collect()
}

/// Every `(agent, topic)` pair some member refuted in window.
///
/// Identified before support is folded, because a member that both supported
/// and refuted one topic is a refuter and not a supporter. Attachment to a
/// standing happens separately, in [`refutations`].
fn refuter_pairs<'a>(live: &[&'a Trace]) -> BTreeSet<(&'a str, &'a TopicId)> {
    live.iter()
        .filter(|trace| trace.kind == TraceKind::Refute && trace.grounded())
        .filter_map(|trace| Some((trace.agent_id()?, trace.topic.as_ref()?)))
        .collect()
}

/// Distinct refuters per advocated topic, in first-refutation order.
///
/// A refutation of a topic nobody advocated is dropped rather than creating a
/// standing: one member must not be able to manufacture an entry for something
/// the room never put on the floor.
fn refutations<'a>(
    live: &[&'a Trace],
    ordered: &[&TopicId],
) -> BTreeMap<&'a TopicId, Vec<&'a str>> {
    let mut refuted: BTreeMap<&'a TopicId, Vec<&'a str>> = BTreeMap::new();
    for trace in live {
        if trace.kind != TraceKind::Refute || !trace.grounded() {
            continue;
        }
        let (Some(topic), Some(agent)) = (trace.topic.as_ref(), trace.agent_id()) else {
            continue;
        };
        if !ordered.contains(&topic) {
            continue;
        }
        let entry = refuted.entry(topic).or_default();
        if !entry.contains(&agent) {
            entry.push(agent);
        }
    }
    refuted
}

/// Return whether a trace's citation chain reaches a stated fact.
///
/// The chain is followed transitively through traces *inside the window* only.
/// A citation that leaves the window is not chased: a member's standing must
/// not depend on how far back it happened to have paged, which is the same
/// locality the window buys everywhere else in this fold.
///
/// The visited set is over sequences, so two supports that cite each other
/// terminate rather than recurring, and the whole pass is linear in the
/// citations present in the window.
fn reaches_evidence(trace: &Trace, by_sequence: &BTreeMap<Sequence, Vec<&Trace>>) -> bool {
    let mut visited: BTreeSet<Sequence> = BTreeSet::new();
    let mut pending: Vec<Sequence> = trace.cites.clone();
    while let Some(sequence) = pending.pop() {
        if !visited.insert(sequence) {
            continue;
        }
        let Some(cited) = by_sequence.get(&sequence) else {
            continue;
        };
        for trace in cited {
            if trace.kind == TraceKind::Evidence {
                return true;
            }
            pending.extend(trace.cites.iter().copied());
        }
    }
    false
}

/// Decide what the standings add up to.
///
/// Exactly one carried topic is [`ConsensusState::Quorum`]; two or more is
/// [`ConsensusState::Deadlocked`], which is a real outcome rather than an
/// error — it is the state cross-inhibition exists to resolve.
#[must_use]
pub fn consensus(standings: &[TopicStanding], policy: &QuorumPolicy) -> ConsensusState {
    let mut carried: Vec<&TopicStanding> = standings
        .iter()
        .filter(|standing| standing.carried(policy))
        .collect();
    match carried.len() {
        0 => ConsensusState::Deliberating,
        1 => ConsensusState::Quorum {
            topic: carried.swap_remove(0).topic.clone(),
        },
        _ => ConsensusState::Deadlocked {
            topics: carried
                .into_iter()
                .map(|standing| standing.topic.clone())
                .collect(),
        },
    }
}
