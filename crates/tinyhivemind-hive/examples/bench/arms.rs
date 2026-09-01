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
    desk::DeskSet,
    responder::{
        ResponderPlan, ResponderRequest, SelectionPolicy, SelectorCandidate, accept_selection,
        responder_plan,
    },
    roster::Roster,
    trace::TopicId,
};

use crate::rng::{Rng, mix};
use crate::run::Host;
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
    let ids = room.member_ids();
    let host = Host::new(&ids);
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
    let request = ResponderRequest {
        message: "We must choose one rollout strategy. Decide.".to_owned(),
        chat: Some(crate::run::DESK_ID.to_owned()),
        mentions: Vec::new(),
        orchestrator_id: ids.first().map_or_else(String::new, |id| (*id).to_owned()),
        selection_policy: SelectionPolicy::Allowed,
    };

    let started = Instant::now();
    let plan = {
        let roster: Roster<'_> = host.roster();
        let desks: DeskSet<'_> = host.desks();
        responder_plan(&request, &roster, &desks, &candidates)
    }
    .map_err(|error| error.to_string())?;

    let responder = match plan {
        ResponderPlan::Decided { decision } => decision.responder_id,
        ResponderPlan::Select { request, fallback } => {
            // A router with no information about the task picks a candidate.
            // Modelling it as a uniform choice is the honest reading: nothing
            // in the ladder knows which member holds the better private signal.
            let mut rng = Rng::seeded(mix(seed, 0x726F_7574));
            let index =
                usize::try_from(rng.below(u32::try_from(request.candidates.len()).unwrap_or(1)))
                    .unwrap_or(0);
            let named = request
                .candidates
                .get(index)
                .map_or_else(String::new, |candidate| candidate.id.clone());
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
            library_time: Duration::ZERO,
        };
    }
    let mut spent = 0_u32;
    while spent < budget {
        let index = usize::try_from(spent).unwrap_or(0) % members;
        if let Some(agent) = room.agents.get(index) {
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
        library_time: Duration::ZERO,
    }
}
