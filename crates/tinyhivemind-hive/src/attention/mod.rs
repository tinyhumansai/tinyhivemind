//! The attention market: every member bids, and exactly one takes the floor.
//!
//! This is Pandemonium's decision demon and the response-threshold model of
//! division of labour, which are the same mechanism arrived at from the AI and
//! the entomology sides. Each member computes an urge from the salience field
//! and its own affinity; [`floor_holder`] takes the argmax.
//!
//! Taking the argmax rather than everyone above threshold is precisely what
//! enforces *one message, one turn*. The bound is not checked after the fact;
//! there is no way to express two winners.
//!
//! Two corrections fold into the bid rather than sitting beside it:
//!
//! - **Dominance.** Equality of conversational turn-taking is one of the few
//!   robust predictors of a group's collective performance. Share here is
//!   measured over *grounded, surviving* contributions rather than raw message
//!   count, because raw count is a proxy an agent inflates for free.
//! - **Repetition.** Once a topic has `repetition_cap` distinct supporters,
//!   restating it scores nothing — the rumour has met enough peers who already
//!   know it. Step repetition is among the most common observed multi-agent
//!   failures, and it is a protocol bug, not a model one.

#[cfg(test)]
mod test;

mod types;

pub use types::{AgentThreshold, Bid, BidContext, BidReason};

use std::collections::BTreeMap;

use crate::{
    error::{Error, Result},
    quorum::TopicStanding,
    salience::{standing, with_relevance},
    trace::{Trace, TraceKind},
};

/// Bonus applied when a trace cited or objected to the member's own message.
const ADDRESSED_BONUS: i64 = 2_000;
/// Bonus applied to a member that has backed neither deadlocked side.
const DISSENT_BONUS: i64 = 1_500;
/// Bonus applied to the least-heard member when the room is lopsided.
const QUIET_BONUS: i64 = 1_000;
/// Penalty applied to a member holding more than `dominance_cap` of the share.
const DOMINANCE_PENALTY: i64 = 3_000;

/// Compute one bid per eligible member.
///
/// A member whose urge does not reach its threshold does not bid at all, so an
/// empty result is a real outcome: nobody has anything to say.
///
/// # Errors
///
/// Returns [`Error::DuplicateAgentThreshold`] when two records name one agent,
/// or a salience failure when the weights are malformed.
pub fn bids(context: &BidContext<'_>) -> Result<Vec<Bid>> {
    let thresholds = index_thresholds(context.thresholds)?;
    let shares = grounded_shares(context);
    let total: u32 = shares.values().sum();
    let quiet = quietest(context.members, &shares);
    let deadlocked = deadlocked_topics(context.standings);

    // Saturation and the recency-and-importance half of salience are
    // properties of a trace, not of the member reading it, so both are folded
    // once here rather than once per member per trace. The arithmetic each
    // member then does is identical to evaluating the whole score inline.
    let mut scored: Vec<(&Trace, i64)> = Vec::new();
    if !context.members.is_empty() {
        for trace in context.traces {
            if is_saturated(trace, context.standings, context.repetition_cap) {
                continue;
            }
            scored.push((trace, standing(trace, context.at, context.weights)?));
        }
    }

    let mut bids = Vec::new();
    for member in context.members {
        let default = AgentThreshold::new(*member, 0);
        let threshold = thresholds.get(*member).copied().unwrap_or(&default);

        let mut urge = 0;
        for (trace, base) in &scored {
            let relevance = threshold.relevance(trace.topic.as_ref());
            urge += with_relevance(*base, context.weights, relevance).0;
        }

        let mut reason = BidReason::Salience;
        if addresses(context.traces, member) {
            urge += ADDRESSED_BONUS;
            reason = BidReason::Addressed;
        } else if !deadlocked.is_empty() && !backs_any(&deadlocked, member) {
            urge += DISSENT_BONUS;
            reason = BidReason::Dissent;
        } else if quiet == Some(*member) && is_lopsided(&shares, total, context.dominance_cap) {
            urge += QUIET_BONUS;
            reason = BidReason::Quiet;
        }

        if dominates(&shares, member, total, context.dominance_cap) {
            urge -= DOMINANCE_PENALTY;
        }

        // The threshold competes rather than merely gating: a member that has
        // just spoken is measurably harder to rouse than one that has been
        // quiet. Gating alone would let the first speaker hold the floor for
        // the whole episode, because urges dwarf any plausible threshold.
        if urge >= threshold.threshold {
            bids.push(Bid {
                agent_id: (*member).to_owned(),
                urge: urge.saturating_sub(threshold.threshold),
                reason,
            });
        }
    }
    Ok(bids)
}

/// Take the single highest bid.
///
/// Ties break by the order the bids were produced in, which is desk order, so
/// the choice is deterministic for a given roster and transcript.
#[must_use]
pub fn floor_holder(bids: &[Bid]) -> Option<&Bid> {
    bids.iter()
        .reduce(|held, next| if next.urge > held.urge { next } else { held })
}

fn index_thresholds(thresholds: &[AgentThreshold]) -> Result<BTreeMap<&str, &AgentThreshold>> {
    let mut indexed = BTreeMap::new();
    for threshold in thresholds {
        if indexed
            .insert(threshold.agent_id.as_str(), threshold)
            .is_some()
        {
            return Err(Error::DuplicateAgentThreshold {
                agent_id: threshold.agent_id.clone(),
            });
        }
    }
    Ok(indexed)
}

/// Count each member's grounded contributions that still survive in a standing.
///
/// Deliberately not a message count: an agent can emit ten ungrounded lines for
/// the price of one, so counting those would reward exactly the behaviour the
/// equality guard exists to damp.
fn grounded_shares<'a>(context: &BidContext<'a>) -> BTreeMap<&'a str, u32> {
    let floor = context.at.0.saturating_sub(u64::from(context.window));
    let mut shares: BTreeMap<&str, u32> =
        context.members.iter().map(|member| (*member, 0)).collect();
    for trace in context.traces {
        if trace.sequence.0 < floor || !trace.grounded() {
            continue;
        }
        let Some(agent) = trace.agent_id() else {
            continue;
        };
        let survives = context
            .standings
            .iter()
            .any(|standing| standing.supporters.iter().any(|held| held == agent));
        if !survives {
            continue;
        }
        if let Some(count) = shares.get_mut(agent) {
            *count += 1;
        }
    }
    shares
}

fn quietest<'a>(members: &[&'a str], shares: &BTreeMap<&str, u32>) -> Option<&'a str> {
    members
        .iter()
        .min_by_key(|member| shares.get(**member).copied().unwrap_or_default())
        .copied()
}

fn is_lopsided(shares: &BTreeMap<&str, u32>, total: u32, cap: u32) -> bool {
    shares.values().any(|share| exceeds(*share, total, cap))
}

fn dominates(shares: &BTreeMap<&str, u32>, member: &str, total: u32, cap: u32) -> bool {
    exceeds(shares.get(member).copied().unwrap_or_default(), total, cap)
}

fn exceeds(share: u32, total: u32, cap: u32) -> bool {
    total > 0 && share * 100 > total * cap
}

fn addresses(traces: &[Trace], member: &str) -> bool {
    let own: Vec<_> = traces
        .iter()
        .filter(|trace| trace.agent_id() == Some(member))
        .map(|trace| trace.sequence)
        .collect();
    traces.iter().any(|trace| {
        trace.agent_id() != Some(member)
            && (trace.target.is_some_and(|target| own.contains(&target))
                || trace.cites.iter().any(|cited| own.contains(cited)))
    })
}

fn deadlocked_topics(standings: &[TopicStanding]) -> Vec<&TopicStanding> {
    if standings.len() < 2 {
        return Vec::new();
    }
    let top = standings
        .iter()
        .map(|standing| standing.supporters.len())
        .max()
        .unwrap_or_default();
    if top == 0 {
        return Vec::new();
    }
    let tied: Vec<&TopicStanding> = standings
        .iter()
        .filter(|standing| standing.supporters.len() == top)
        .collect();
    if tied.len() < 2 { Vec::new() } else { tied }
}

fn backs_any(deadlocked: &[&TopicStanding], member: &str) -> bool {
    deadlocked
        .iter()
        .any(|standing| standing.supporters.iter().any(|held| held == member))
}

fn is_saturated(trace: &Trace, standings: &[TopicStanding], cap: u32) -> bool {
    if cap == 0 || trace.kind != TraceKind::Support {
        return false;
    }
    let Some(topic) = trace.topic.as_ref() else {
        return false;
    };
    standings.iter().any(|standing| {
        &standing.topic == topic
            && u32::try_from(standing.supporters.len()).is_ok_and(|count| count >= cap)
    })
}
