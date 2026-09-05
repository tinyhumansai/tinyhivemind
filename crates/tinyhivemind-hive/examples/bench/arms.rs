//! The two control arms the deliberation is measured against.
//!
//! A multi-agent result is close to meaningless without a matched-budget
//! control: almost every positive finding in the literature is confounded by
//! the extra compute the multi-agent arm spent. So the benchmark runs three
//! arms over the *same* rooms and the same private evaluations:
//!
//! - **ladder** — today's behaviour. One message selects exactly one responder
//!   off the deterministic ladder in `tinyhivemind`, that agent answers, and the
//!   interaction ends. One turn.
//! - **vote** — self-consistency at a matched turn budget. The same number of
//!   turns the episode was allowed, each spent on one member's independent
//!   first answer, decided by plurality. No member ever sees another's.
//! - **hive** — the deliberation episode, at the same budget.
//!
//! `vote` is the honest control, and it is a strong one: independent sampling
//! plus a plurality already recovers a lot of what a room is for.

use std::time::{Duration, Instant};

use tinyhivemind_hive::{
    Directory, EpisodePolicy,
    desk::DeskSet,
    responder::{
        ResponderPlan, ResponderRequest, SelectionPolicy, SelectorCandidate, accept_selection,
        responder_plan,
    },
    roster::Roster,
    trace::TopicId,
};

use crate::federation::Federation;
use crate::rng::{Rng, mix};
use crate::run::{Host, Participant, drive};
use crate::sim::Room;

/// What one control arm decided, and what it spent.
#[derive(Clone, Debug)]
pub(crate) struct ArmReport {
    /// The option it settled on.
    pub(crate) decided: Option<TopicId>,
    /// Whether that option is the genuinely best one.
    pub(crate) correct: bool,
    /// Turns spent.
    pub(crate) turns: u32,
    /// What those turns cost, in [`crate::run::Participant::cost_unit`]
    /// units. One unit per turn unless the room was generated with
    /// `--cost-tiers` and a specialist answered.
    pub(crate) cost_units: u64,
    /// Whether the responder ladder picked the room's expert on the topic
    /// the answer turns on. `None` for an arm that never consults the
    /// ladder, and for a room that names no such expert.
    pub(crate) routed_right: Option<bool>,
    /// Time spent inside the library.
    pub(crate) library_time: Duration,
}

/// Route one message through the real responder ladder and take that
/// responder's unaided answer.
///
/// The selector rung is exercised rather than skipped: the ladder returns a
/// bounded [`ResponderPlan::Select`], a simulated router names one of the
/// candidates it was given, and [`accept_selection`] validates that name the
/// way a host would validate a model's output.
///
/// # Errors
///
/// Returns the ladder's own error text for a malformed snapshot.
pub(crate) fn run_ladder(room: &Room, seed: u64) -> Result<ArmReport, String> {
    // A router with no information about the task picks a candidate. Modelling
    // it as a uniform choice is the honest reading: nothing in the ladder
    // knows which member holds the better private signal.
    let uninformed = |candidates: &[SelectorCandidate]| {
        let mut rng = Rng::seeded(mix(seed, 0x726F_7574));
        let index =
            usize::try_from(rng.below(u32::try_from(candidates.len()).unwrap_or(1))).unwrap_or(0);
        candidates
            .get(index)
            .map_or_else(String::new, |candidate| candidate.id.clone())
    };
    let candidates: Vec<SelectorCandidate> = room
        .agents
        .iter()
        .map(|agent| SelectorCandidate {
            id: agent.id.clone(),
            label: agent.id.clone(),
            role: format!("{:?}", agent.role).to_lowercase(),
            description: None,
        })
        .collect();
    route(
        room,
        &candidates,
        "We must choose one rollout strategy. Decide.",
        &uninformed,
    )
}

/// The same ladder, given a directory the room *earned* in earlier episodes.
///
/// Three things change and nothing else does. Each candidate carries a
/// `description` built from that member's own directory lines, so the
/// selector sees what the transcript says the member has grounded. The
/// request names `room.truth` as the topic the call turns on. And the
/// simulated router, instead of drawing uniformly, reads the descriptions it
/// was given and names the candidate carrying the most weight on that topic.
/// It is still validated through the real [`accept_selection`], so a router
/// that named a non-candidate would fall back exactly as the uninformed one
/// does.
///
/// **This arm's score measures the leak as much as the mechanism.** The topic
/// it names *is* the correct option, so a directory that records who deposited
/// a reading of `#truth` is already halfway to naming the winner — the
/// heaviest holder of that topic is usually the member whose favourite is that
/// option. Read its number knowing the topic was named; the README's
/// "`ladder+dir` on a uniform room is an artifact, not a result" paragraph
/// gives the size of the swing.
///
/// It takes a [`Directory`] and never [`Room::experts`]: routing on the room's
/// own scoring data would measure nothing but the harness handing itself the
/// answer.
///
/// # Errors
///
/// Returns the ladder's own error text for a malformed snapshot.
pub(crate) fn run_ladder_directed(
    room: &Room,
    known: &Directory,
    seed: u64,
) -> Result<ArmReport, String> {
    let topic = room.truth.clone();
    let candidates: Vec<SelectorCandidate> = room
        .agents
        .iter()
        .map(|agent| SelectorCandidate {
            id: agent.id.clone(),
            label: agent.id.clone(),
            role: format!("{:?}", agent.role).to_lowercase(),
            description: describe(known, &agent.id),
        })
        .collect();
    let message =
        format!("We must choose one rollout strategy. The call turns on #{topic}. Decide.");
    let informed = |candidates: &[SelectorCandidate]| {
        // A router that read the descriptions: it picks the candidate whose
        // own description carries the most weight on the named topic. Ties,
        // and a room where nobody has grounded the topic at all, fall back to
        // the uninformed draw rather than to the first seat in desk order.
        let mut best: Option<(&str, i64)> = None;
        for candidate in candidates {
            let Some(weight) = described_weight(candidate.description.as_deref(), &topic) else {
                continue;
            };
            if weight > 0 && best.is_none_or(|(_, held)| weight > held) {
                best = Some((candidate.id.as_str(), weight));
            }
        }
        if let Some((id, _)) = best {
            return id.to_owned();
        }
        let mut rng = Rng::seeded(mix(seed, 0x726F_7574));
        let index =
            usize::try_from(rng.below(u32::try_from(candidates.len()).unwrap_or(1))).unwrap_or(0);
        candidates
            .get(index)
            .map_or_else(String::new, |candidate| candidate.id.clone())
    };
    route(room, &candidates, &message, &informed)
}

/// One member's directory lines, as the `description` a selector reads.
///
/// `None` for a member the directory never named, which is the honest shape:
/// a candidate the transcript says nothing about should not be described as
/// knowing nothing, it should be described not at all.
fn describe(known: &Directory, agent_id: &str) -> Option<String> {
    let mut held: Vec<&tinyhivemind_hive::DirectoryEntry> = known
        .entries()
        .iter()
        .filter(|entry| entry.agent_id == agent_id && entry.weight > 0)
        .collect();
    if held.is_empty() {
        return None;
    }
    held.sort_by_key(|entry| std::cmp::Reverse(entry.weight));
    let described: Vec<String> = held
        .iter()
        .map(|entry| format!("#{} {}", entry.topic, entry.weight))
        .collect();
    Some(format!("has grounded: {}", described.join(", ")))
}

/// Read one topic's weight back out of a description [`describe`] wrote.
///
/// The router only ever sees the candidate list, so it reads the weight out
/// of the prose it was given rather than consulting the directory again --
/// which is what makes this a router that read the descriptions rather than
/// one holding a private index.
fn described_weight(description: Option<&str>, topic: &TopicId) -> Option<i64> {
    let description = description?;
    let needle = format!("#{topic} ");
    let at = description.find(&needle)? + needle.len();
    description[at..]
        .split(|character: char| !character.is_ascii_digit())
        .next()
        .and_then(|digits| digits.parse::<i64>().ok())
}

/// Walk the real responder ladder once and take the selected member's
/// unaided answer.
///
/// `select` stands in for the host's router on the [`ResponderPlan::Select`]
/// rung: it is handed exactly the candidates the ladder bounded the choice
/// to, and whatever it names is validated through [`accept_selection`] the
/// way a host would validate a model's output.
fn route(
    room: &Room,
    candidates: &[SelectorCandidate],
    message: &str,
    select: &dyn Fn(&[SelectorCandidate]) -> String,
) -> Result<ArmReport, String> {
    let ids = room.member_ids();
    let host = Host::new(&ids);
    let request = ResponderRequest {
        message: message.to_owned(),
        chat: Some(crate::run::DESK_ID.to_owned()),
        mentions: Vec::new(),
        orchestrator_id: ids.first().map_or_else(String::new, |id| (*id).to_owned()),
        selection_policy: SelectionPolicy::Allowed,
    };

    let started = Instant::now();
    let plan = {
        let roster: Roster<'_> = host.roster();
        let desks: DeskSet<'_> = host.desks();
        responder_plan(&request, &roster, &desks, candidates)
    }
    .map_err(|error| error.to_string())?;

    let responder = match plan {
        ResponderPlan::Decided { decision } => decision.responder_id,
        ResponderPlan::Select { request, fallback } => {
            let named = select(&request.candidates);
            accept_selection(&named, &request.candidates).unwrap_or(fallback.responder_id)
        }
    };
    let library_time = started.elapsed();

    let decided = room
        .agents
        .iter()
        .find(|agent| agent.id == responder)
        .map(|agent| agent.favourite().clone());
    let correct = decided.as_ref() == Some(&room.truth);
    Ok(ArmReport {
        decided,
        correct,
        turns: 1,
        cost_units: u64::from(room.cost_of(&responder)),
        routed_right: room.deciding_expert().map(|held| held == responder),
        library_time,
    })
}

/// Spend a matched budget on independent answers and take the plurality.
///
/// Every member answers from its own private evaluation alone, having seen
/// nothing, so this is the pure aggregation baseline: no deliberation, no
/// influence, no cascade.
pub(crate) fn run_vote(room: &Room, budget: u32) -> ArmReport {
    let mut tally: Vec<(TopicId, u32)> = Vec::new();
    let members = room.agents.len();
    if members == 0 || budget == 0 {
        return ArmReport {
            decided: None,
            correct: false,
            turns: 0,
            cost_units: 0,
            routed_right: None,
            library_time: Duration::ZERO,
        };
    }
    let mut spent = 0_u32;
    let mut cost_units = 0_u64;
    while spent < budget {
        let index = usize::try_from(spent).unwrap_or(0) % members;
        if let Some(agent) = room.agents.get(index) {
            cost_units = cost_units.saturating_add(u64::from(room.cost_of(&agent.id)));
            let favourite = agent.favourite().clone();
            if let Some(entry) = tally.iter_mut().find(|(topic, _)| *topic == favourite) {
                entry.1 = entry.1.saturating_add(1);
            } else {
                tally.push((favourite, 1));
            }
        }
        spent = spent.saturating_add(1);
    }

    // A plurality, ties broken by first appearance, which is desk order.
    let decided = tally
        .iter()
        .max_by_key(|(_, count)| *count)
        .map(|(topic, _)| topic.clone());
    let correct = decided.as_ref() == Some(&room.truth);
    ArmReport {
        decided,
        correct,
        turns: spent,
        cost_units,
        routed_right: None,
        library_time: Duration::ZERO,
    }
}

/// Poll every member of a federation independently and take the plurality.
///
/// This is the matched-budget control for the swarm: one turn per member, no
/// member seeing any other, exactly as many agent invocations as there are
/// agents. It is a strong control on an ordinary problem and a weak one on a
/// federated hidden profile, and saying which is which is the point of running
/// it.
pub(crate) fn run_federated_vote(federation: &Federation) -> ArmReport {
    let mut tally: Vec<(&TopicId, u32)> = Vec::new();
    for agent in &federation.agents {
        let pick = agent.favourite();
        match tally.iter_mut().find(|(topic, _)| *topic == pick) {
            Some(entry) => entry.1 = entry.1.saturating_add(1),
            None => tally.push((pick, 1)),
        }
    }
    let decided = plurality(&tally);
    let turns = u32::try_from(federation.agents.len()).unwrap_or(u32::MAX);
    ArmReport {
        correct: decided.as_ref() == Some(&federation.truth),
        decided,
        turns,
        cost_units: u64::from(turns),
        routed_right: None,
        library_time: Duration::ZERO,
    }
}

/// Put every member of every desk on one desk and deliberate there.
///
/// This is the control that asks whether the *channel structure* costs
/// anything. It removes the boundary rather than crossing it, which no real
/// organisation can do, and it is given the whole federation's turn budget.
///
/// # Errors
///
/// Returns the library's own error text for a malformed snapshot.
pub(crate) fn run_merged(
    federation: &Federation,
    policy: &EpisodePolicy,
    task: &str,
) -> Result<ArmReport, String> {
    let mut agents = federation.agents.clone();
    for agent in &mut agents {
        agent.set_quorum(policy.quorum);
    }
    let ids: Vec<&str> = federation
        .agents
        .iter()
        .map(|agent| agent.id.as_str())
        .collect();
    let mut participants: Vec<&mut dyn Participant> = agents
        .iter_mut()
        .map(|agent| agent as &mut dyn Participant)
        .collect();
    let report = drive(&ids, &mut participants, policy, task, false)?;
    Ok(ArmReport {
        correct: report.decided.as_ref() == Some(&federation.truth),
        decided: report.decided,
        turns: report.turns,
        cost_units: report.cost_units,
        routed_right: None,
        library_time: report.library_time,
    })
}

/// The single option with the most votes, or `None` when the tally is tied.
fn plurality(tally: &[(&TopicId, u32)]) -> Option<TopicId> {
    let most = tally.iter().map(|(_, count)| *count).max()?;
    let mut leaders = tally.iter().filter(|(_, count)| *count == most);
    let leader = leaders.next()?;
    if leaders.next().is_some() {
        return None;
    }
    Some(leader.0.clone())
}
