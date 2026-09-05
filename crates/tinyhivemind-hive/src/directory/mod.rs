//! Who knows what, folded from grounded deposits and the citations they drew.
//!
//! This is Wegner's transactive memory: a group's memory is the *directory*,
//! not the contents. Lewis names three factors, of which two can be estimated
//! from an attributed transcript — **specialisation**, whose deposits cluster
//! on a topic, and **credibility**, whose deposits other members build on.
//! Hollingshead separates a *diffuse* cue such as a role label from a
//! *specific* cue such as observed experience, and finds the diffuse cue
//! matters less as a team accumulates shared history. The host's
//! [`AgentThreshold::affinity`] is the diffuse cue and enters as a prior; the
//! two folded terms are the specific ones.
//!
//! Nothing here is stored. [`directory`] is a fold over traces the caller
//! already holds, refolded on every step, order-independent and idempotent on
//! `(sequence, offset)` exactly as [`standings`] is. An iterated per-turn
//! update would not be commutative, and a stored one would be a second journal
//! that has to be invalidated whenever the transcript is re-paged.
//!
//! Speech is deliberately *not* the estimator. Talking more, ungrounded, earns
//! nothing: only a stated fact or a grounded position deposits, and only
//! *other* members' citations earn credibility. That bounds — and does not
//! remove — the circularity every transcript-folded expertise estimate has,
//! where who spoke becomes who is thought to know. See `README.md` and
//! [ADR 0007].
//!
//! [`AgentThreshold::affinity`]: crate::attention::AgentThreshold::affinity
//! [`standings`]: crate::quorum::standings
//! [ADR 0007]: https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/adr/0007-the-directory-is-folded-from-citations.md

#[cfg(test)]
mod test;

mod types;

pub use types::{Directory, DirectoryEntry, DirectoryPolicy, WEIGHT_CEILING};

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    attention::AgentThreshold,
    error::{Error, Result},
    salience::{SCALE, decay},
    trace::{TopicId, Trace, TraceKind},
};
use tinyhivemind::Sequence;

/// What a stated fact puts on the floor, in thousandths.
const EVIDENCE_DEPOSIT: i64 = 1_000;
/// What a grounded position puts on the floor, in thousandths.
///
/// Less than a fact, because a position is a conclusion with grounds attached
/// rather than the grounds themselves.
const POSITION_DEPOSIT: i64 = 600;
/// What one other member's citation is worth, in thousandths.
const CITATION_CREDIT: i64 = 1_000;
/// Tenths, since `discredit` is expressed in tenths of one citation.
const DISCREDIT_SCALE: i64 = 10_000;
/// Declared relevance runs `0..=100`; the weight terms run in thousandths.
const PRIOR_SCALE: i64 = 10;
/// The unit `specialisation`, `credibility` and `prior` are expressed in.
///
/// The same number as [`PRIOR_SCALE`] and a different quantity: this one
/// divides the policy's weights back out of the combined total, and would
/// still be ten if declared relevance were rescaled tomorrow.
const TENTHS: i64 = 10;

/// Fold the transcript into an estimate of who knows what.
///
/// `priors` are the same [`AgentThreshold`] records the attention market
/// reads. Only their `affinity` is used here, and only where a member actually
/// declared a topic — an undeclared topic contributes nothing rather than the
/// neutral 50 [`AgentThreshold::relevance`] substitutes, because "unknown" and
/// "moderately expert" are different claims.
///
/// Entries come back in `(topic, agent_id)` order. A pair every term scored
/// zero on is dropped, so the result names only holders.
///
/// # Errors
///
/// Returns [`Error::ZeroDirectoryHalfLife`] or [`Error::ZeroDirectoryWindow`]
/// when the policy would make the estimate meaningless, or
/// [`Error::DuplicateAgentThreshold`] when two prior records name one agent.
///
/// # Example
///
/// ```
/// use tinyhivemind_hive::{
///     directory::{directory, DirectoryPolicy},
///     trace::read,
///     Sequence, SessionAuthor, SessionMessage,
/// };
///
/// fn said(sequence: u64, id: &str, content: &str) -> SessionMessage {
///     SessionMessage {
///         sequence: Sequence(sequence),
///         author: SessionAuthor::Agent { id: id.into(), label: id.into() },
///         content: content.into(),
///     }
/// }
///
/// let transcript = [
///     said(1, "archivist", "!evidence #pool The pool caps at twenty."),
///     said(2, "planner", "!propose #pool ^1 Raise the cap."),
/// ];
/// let folded = directory(&read(&transcript), Sequence(2), &DirectoryPolicy::DEFAULT, &[])?;
///
/// // The archivist stated the fact and the planner built on it.
/// assert_eq!(folded.top(&"pool".into()).map(|entry| entry.agent_id.as_str()), Some("archivist"));
/// assert!(folded.knows("archivist", &"pool".into(), &DirectoryPolicy::DEFAULT));
/// # Ok::<(), tinyhivemind_hive::error::Error>(())
/// ```
pub fn directory(
    traces: &[Trace],
    at: Sequence,
    policy: &DirectoryPolicy,
    priors: &[AgentThreshold],
) -> Result<Directory> {
    if policy.half_life == 0 {
        return Err(Error::ZeroDirectoryHalfLife);
    }
    if policy.window == 0 {
        return Err(Error::ZeroDirectoryWindow);
    }
    let indexed = index_priors(priors)?;

    let live = live_traces(traces, at, policy.window);
    let deposits = deposits_by_sequence(&live);

    let specialisation = specialisation(&live, at, policy);
    let credibility = credibility(&live, &deposits, at, policy);
    let deferred = deferrals(&live);

    let mut entries = Vec::new();
    for (agent, topic) in candidates(&specialisation, &credibility, priors) {
        let specialisation = specialisation.get(&(agent, topic)).copied().unwrap_or(0);
        let credibility = credibility
            .get(&(agent, topic))
            .copied()
            .unwrap_or(0)
            .max(0);
        let prior = indexed
            .get(agent)
            .and_then(|threshold| threshold.declared_relevance(topic))
            .map_or(0, |declared| i64::from(declared.min(100)) * PRIOR_SCALE);
        if specialisation == 0 && credibility == 0 && prior == 0 {
            continue;
        }
        // A member that said "not mine" outranks anything the fold inferred
        // about them, so the deferral zeroes the weight rather than debiting
        // it. The two components stay visible, because the transcript still
        // records what the member deposited.
        let weight = if deferred.contains(&(agent, topic)) {
            0
        } else {
            weigh(specialisation, credibility, prior, policy)
        };
        entries.push(DirectoryEntry {
            agent_id: agent.to_owned(),
            topic: topic.clone(),
            specialisation,
            credibility,
            weight,
        });
    }
    entries.sort_by(|left, right| {
        left.topic
            .cmp(&right.topic)
            .then_with(|| left.agent_id.cmp(&right.agent_id))
    });
    Ok(Directory::new(entries))
}

/// Combine the three terms into one clamped weight.
fn weigh(specialisation: i64, credibility: i64, prior: i64, policy: &DirectoryPolicy) -> i64 {
    let total = specialisation
        .saturating_mul(i64::from(policy.specialisation))
        .saturating_add(credibility.saturating_mul(i64::from(policy.credibility)))
        .saturating_add(prior.saturating_mul(i64::from(policy.prior)));
    (total / TENTHS).clamp(0, WEIGHT_CEILING)
}

/// Index prior records by agent, rejecting a repeat.
fn index_priors(priors: &[AgentThreshold]) -> Result<BTreeMap<&str, &AgentThreshold>> {
    let mut indexed = BTreeMap::new();
    for prior in priors {
        if indexed.insert(prior.agent_id.as_str(), prior).is_some() {
            return Err(Error::DuplicateAgentThreshold {
                agent_id: prior.agent_id.clone(),
            });
        }
    }
    Ok(indexed)
}

/// The window's traces, sorted and deduplicated by `(sequence, offset)`.
///
/// Identical to what [`standings`] does, and for the same reason: a trace is
/// addressed by where it was authored, so folding on that address is what
/// makes the result commutative and idempotent.
///
/// [`standings`]: crate::quorum::standings
fn live_traces(traces: &[Trace], at: Sequence, window: u32) -> Vec<&Trace> {
    let floor = at.0.saturating_sub(u64::from(window));
    let mut live: Vec<&Trace> = traces
        .iter()
        .filter(|trace| trace.sequence.0 >= floor && trace.sequence <= at)
        .collect();
    live.sort_by_key(|trace| (trace.sequence, trace.offset));
    live.dedup_by_key(|trace| (trace.sequence, trace.offset));
    live
}

/// What one trace put on the floor that another member could build on.
///
/// A conclusion offered without grounds deposits nothing, which is the same
/// second-class treatment `require_grounded` gives support, for the same
/// reason: it is the cheapest thing to emit and would be the cheapest way to
/// inflate an estimate.
fn deposit(trace: &Trace) -> i64 {
    match trace.kind {
        TraceKind::Evidence => EVIDENCE_DEPOSIT,
        TraceKind::Propose | TraceKind::Support | TraceKind::Refute if trace.grounded() => {
            POSITION_DEPOSIT
        }
        TraceKind::Propose
        | TraceKind::Support
        | TraceKind::Refute
        | TraceKind::Object
        | TraceKind::Question
        | TraceKind::Commit
        | TraceKind::Defer => 0,
    }
}

/// One member's deposit at one sequence, and the topic it named.
#[derive(Clone, Copy)]
struct Deposit<'a> {
    agent: &'a str,
    topic: Option<&'a TopicId>,
}

/// Index every live deposit by the sequence a citation would name.
fn deposits_by_sequence<'a>(live: &[&'a Trace]) -> BTreeMap<Sequence, Vec<Deposit<'a>>> {
    let mut deposits: BTreeMap<Sequence, Vec<Deposit<'a>>> = BTreeMap::new();
    for trace in live {
        let Some(agent) = trace.agent_id() else {
            continue;
        };
        if deposit(trace) == 0 {
            continue;
        }
        deposits.entry(trace.sequence).or_default().push(Deposit {
            agent,
            topic: trace.topic.as_ref(),
        });
    }
    deposits
}

/// Decayed weight of each member's own topiced deposits.
fn specialisation<'a>(
    live: &[&'a Trace],
    at: Sequence,
    policy: &DirectoryPolicy,
) -> BTreeMap<(&'a str, &'a TopicId), i64> {
    let mut scored: BTreeMap<(&str, &TopicId), i64> = BTreeMap::new();
    for trace in live {
        let (Some(agent), Some(topic)) = (trace.agent_id(), trace.topic.as_ref()) else {
            continue;
        };
        let value = deposit(trace);
        if value == 0 {
            continue;
        }
        let entry = scored.entry((agent, topic)).or_default();
        *entry =
            entry.saturating_add(value.saturating_mul(decayed(at, trace.sequence, policy)) / SCALE);
    }
    scored
}

/// Which topics each deposit was read as being about.
///
/// Its own `#topic` when it named one, plus the topic of every *other*
/// member's trace that cited it. A refutation counts: a member killing a topic
/// with someone else's fact is the room using that fact, on that topic.
type Attribution<'a> = BTreeMap<(&'a str, Sequence), BTreeSet<&'a TopicId>>;

/// Decayed citations by other members, less the objections aimed at them.
///
/// The attribution map is built in the same pass rather than separately: an
/// objection debits the topics the objected-to deposit was read as being
/// about, and that reading is exactly what the citers established.
fn credibility<'a>(
    live: &[&'a Trace],
    deposits: &BTreeMap<Sequence, Vec<Deposit<'a>>>,
    at: Sequence,
    policy: &DirectoryPolicy,
) -> BTreeMap<(&'a str, &'a TopicId), i64> {
    let mut attributed: Attribution<'a> = BTreeMap::new();
    for (sequence, holders) in deposits {
        for holder in holders {
            let entry = attributed.entry((holder.agent, *sequence)).or_default();
            if let Some(topic) = holder.topic {
                entry.insert(topic);
            }
        }
    }

    let mut scored: BTreeMap<(&str, &TopicId), i64> = BTreeMap::new();
    for citer in live {
        let (Some(author), Some(topic)) = (citer.agent_id(), citer.topic.as_ref()) else {
            continue;
        };
        let credit = CITATION_CREDIT.saturating_mul(decayed(at, citer.sequence, policy)) / SCALE;
        // One citing trace credits each cited member once, however many of
        // that member's deposits it happens to name.
        let mut credited: BTreeSet<&str> = BTreeSet::new();
        for cited in &citer.cites {
            let Some(holders) = deposits.get(cited) else {
                continue;
            };
            for holder in holders {
                // Citing yourself earns nothing. Credibility is a judgement
                // other members made, and this is the whole reason it is a
                // separate term from specialisation.
                if holder.agent == author {
                    continue;
                }
                attributed
                    .entry((holder.agent, *cited))
                    .or_default()
                    .insert(topic);
                if credited.insert(holder.agent) {
                    let entry = scored.entry((holder.agent, topic)).or_default();
                    *entry = entry.saturating_add(credit);
                }
            }
        }
    }

    for objection in live {
        if objection.kind != TraceKind::Object {
            continue;
        }
        let (Some(author), Some(target)) = (objection.agent_id(), objection.target) else {
            continue;
        };
        let Some(holders) = deposits.get(&target) else {
            continue;
        };
        let debit = i64::from(policy.discredit)
            .saturating_mul(CITATION_CREDIT)
            .saturating_mul(decayed(at, objection.sequence, policy))
            / DISCREDIT_SCALE;
        let objected: BTreeSet<&str> = holders
            .iter()
            .map(|holder| holder.agent)
            .filter(|agent| *agent != author)
            .collect();
        for agent in objected {
            let Some(topics) = attributed.get(&(agent, target)) else {
                continue;
            };
            for topic in topics {
                let entry = scored.entry((agent, *topic)).or_default();
                *entry = entry.saturating_sub(debit);
            }
        }
    }
    scored
}

/// Every `(agent, topic)` a member deferred in window.
fn deferrals<'a>(live: &[&'a Trace]) -> BTreeSet<(&'a str, &'a TopicId)> {
    live.iter()
        .filter(|trace| trace.kind == TraceKind::Defer)
        .filter_map(|trace| Some((trace.agent_id()?, trace.topic.as_ref()?)))
        .collect()
}

/// Every pair any term reaches, including one a host declared and nobody has
/// spoken to yet.
fn candidates<'a>(
    specialisation: &BTreeMap<(&'a str, &'a TopicId), i64>,
    credibility: &BTreeMap<(&'a str, &'a TopicId), i64>,
    priors: &'a [AgentThreshold],
) -> BTreeSet<(&'a str, &'a TopicId)> {
    let mut candidates: BTreeSet<(&str, &TopicId)> = specialisation.keys().copied().collect();
    candidates.extend(credibility.keys().copied());
    for prior in priors {
        for (topic, _) in &prior.affinity {
            candidates.insert((prior.agent_id.as_str(), topic));
        }
    }
    candidates
}

/// The salience field's decay curve, applied to a deposit's age.
fn decayed(at: Sequence, sequence: Sequence, policy: &DirectoryPolicy) -> i64 {
    decay(at.0.saturating_sub(sequence.0), policy.half_life)
}
