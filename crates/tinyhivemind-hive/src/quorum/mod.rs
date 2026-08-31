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

#[cfg(test)]
mod test;

mod types;

pub use types::{ConsensusState, QuorumPolicy, TopicStanding};

use std::collections::BTreeMap;

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
/// Objections are applied after all support, so the result does not depend on
/// the order traces arrived in.
///
/// # Errors
///
/// Returns [`Error::ZeroQuorumThreshold`] or [`Error::ZeroQuorumWindow`] when
/// the policy would make the count meaningless.
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
        if policy.require_grounded && trace.kind == TraceKind::Support && !trace.grounded() {
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

    let mut silenced: BTreeMap<&TopicId, Vec<&str>> = BTreeMap::new();
    for trace in &live {
        if trace.kind != TraceKind::Object {
            continue;
        }
        if policy.require_grounded && !trace.grounded() {
            continue;
        }
        let Some(target) = trace.target else { continue };
        let Some(advocacies) = advocacy.get(&target) else {
            continue;
        };
        for (advocate, topic) in advocacies {
            // An objection cannot silence its own author; that would let an
            // agent retract another's support by objecting to itself.
            if trace.agent_id() == Some(*advocate) {
                continue;
            }
            let entry = silenced.entry(topic).or_default();
            if !entry.contains(advocate) {
                entry.push(advocate);
            }
        }
    }

    Ok(ordered
        .into_iter()
        .map(|topic| {
            let silenced = silenced.remove(&topic).unwrap_or_default();
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
                support,
            }
        })
        .collect())
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
