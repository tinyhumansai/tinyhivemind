//! Room and member generation.
//!
//! [`super::Room::generate_with`] draws a room's truth, its per-topic expert
//! assignment, and every member's private evaluations from one seed. The draw
//! itself -- the expertise stream, the per-member noise, and the one-room
//! reproducibility self-check -- is substantial enough, and separable enough
//! from what a room and a member *are*, to earn its own module rather than
//! growing `mod.rs` past what a reader can hold in mind.

use tinyhivemind_hive::trace::TopicId;

use super::{
    DECOY_QUALITY, EXPERT_NOISE_DIVISOR, Expertise, HIDDEN_LIFT, LAY_NOISE_PERCENT, Role,
    SELFCHECK_ROOM_SEED, SPECIALIST_COST_UNIT, SimAgent, TRUE_QUALITY,
};
use crate::rng::{Rng, mix};

/// Draw the per-topic expert assignment and, for a hidden profile, its
/// decisive member and its planted decoy, from a stream seeded independently
/// of every per-member draw. Under `Expertise::Uniform` this never asks the
/// stream for a value at all.
pub(super) fn draw_expertise(
    expert_rng: &mut Rng,
    agent_count: usize,
    topic_count: usize,
    truth_index: usize,
    expertise: Expertise,
) -> (Vec<Option<usize>>, Option<usize>, Option<usize>) {
    // Per-topic expert, by index into `names`; `None` where no member
    // specialises in that topic.
    let mut expert_of: Vec<Option<usize>> = vec![None; topic_count];
    let mut decisive_index: Option<usize> = None;
    let mut planted_index: Option<usize> = None;

    match expertise {
        Expertise::Uniform => {}
        Expertise::Specialists { count } => {
            // Distinct (member, topic) pairs: information is redistributed
            // onto a specialist, not created, so neither a member nor a
            // topic is drawn twice.
            let count = count.min(agent_count).min(topic_count);
            let mut members_left: Vec<usize> = (0..agent_count).collect();
            let mut topics_left: Vec<usize> = (0..topic_count).collect();
            for _ in 0..count {
                if members_left.is_empty() || topics_left.is_empty() {
                    break;
                }
                let member_pick = usize::try_from(
                    expert_rng.below(u32::try_from(members_left.len()).unwrap_or(1)),
                )
                .unwrap_or(0);
                let member = members_left.remove(member_pick);
                let topic_pick = usize::try_from(
                    expert_rng.below(u32::try_from(topics_left.len()).unwrap_or(1)),
                )
                .unwrap_or(0);
                let topic = topics_left.remove(topic_pick);
                expert_of[topic] = Some(member);
            }
        }
        Expertise::Roles { owner } => {
            // Every topic, one owner. Nothing is drawn from the stream, so a
            // room built this way is a pure function of its seed and its
            // owner index — which is what lets `--facets` say who is
            // responsible for what without asking the weather.
            let owner = owner.min(agent_count.saturating_sub(1));
            expert_of.fill(Some(owner));
        }
        Expertise::HiddenProfile => {
            let candidates: Vec<usize> = (0..topic_count)
                .filter(|index| *index != truth_index)
                .collect();
            if !candidates.is_empty() {
                let pick =
                    usize::try_from(expert_rng.below(u32::try_from(candidates.len()).unwrap_or(1)))
                        .unwrap_or(0);
                planted_index = candidates.get(pick).copied();
            }
            decisive_index = Some(
                usize::try_from(expert_rng.below(u32::try_from(agent_count).unwrap_or(1)))
                    .unwrap_or(0),
            );
        }
    }
    (expert_of, decisive_index, planted_index)
}

/// What every per-member agent builder below needs, gathered so adding one
/// does not mean adding another function parameter.
pub(super) struct MemberDraw<'a> {
    /// The room's seed, mixed per member to draw that member's own noise.
    pub(super) seed: u64,
    /// The room's option names, in draw order.
    pub(super) names: &'a [TopicId],
    /// The option that is genuinely best.
    pub(super) truth: &'a TopicId,
    /// Half-width of the uniform error on each private evaluation.
    pub(super) noise: u32,
}

/// Build one member under `Expertise::Specialists`.
pub(super) fn specialist_agent(
    id: &str,
    role: Role,
    index: usize,
    draw: &MemberDraw<'_>,
    expert_of: &[Option<usize>],
    cost_tiers: bool,
) -> SimAgent {
    let mut draws = Rng::seeded(mix(draw.seed, index as u64));
    let evals: Vec<(TopicId, i32)> = draw
        .names
        .iter()
        .enumerate()
        .map(|(topic_index, topic)| {
            let base = if topic == draw.truth {
                TRUE_QUALITY
            } else {
                DECOY_QUALITY
            };
            let half_width = match expert_of[topic_index] {
                Some(expert) if expert == index => draw.noise / EXPERT_NOISE_DIVISOR,
                Some(_) => draw.noise.saturating_mul(LAY_NOISE_PERCENT) / 100,
                None => draw.noise,
            };
            (topic.clone(), base + draws.centered(half_width))
        })
        .collect();
    let mut agent = SimAgent::assembled(id, role, draw.seed, index, evals);
    agent.specialty = expert_of
        .iter()
        .position(|holder| *holder == Some(index))
        .and_then(|topic_index| draw.names.get(topic_index).cloned());
    agent.expert_elsewhere = expert_of
        .iter()
        .enumerate()
        .filter_map(|(topic_index, holder)| match holder {
            Some(holder) if *holder != index => draw.names.get(topic_index).cloned(),
            _ => None,
        })
        .collect();
    if cost_tiers && agent.specialty.is_some() {
        agent.cost_unit = SPECIALIST_COST_UNIT;
    }
    agent
}

/// Build one member under `Expertise::Roles`.
///
/// The owner of a facet is *better informed*, not oracular. It reads every
/// option at the room's base noise — exactly what a member of a uniform room
/// reads at — and everybody else reads at [`LAY_NOISE_PERCENT`] of it, the
/// same widening a lay member takes on somebody else's specialty.
///
/// Deliberately weaker than [`specialist_agent`], which divides the owner's
/// noise by [`EXPERT_NOISE_DIVISOR`] as well. At the noise this harness runs
/// at that division puts an expert's error far inside the sixty-point gap
/// between the true option and a decoy, so an owner would answer its own facet
/// correctly essentially always and `--facets` would measure the constant
/// rather than the room. Information is redistributed here, never created: the
/// owner's read is unchanged from a uniform room's and only the room around it
/// widens.
pub(super) fn roles_agent(
    id: &str,
    role: Role,
    index: usize,
    draw: &MemberDraw<'_>,
    expert_of: &[Option<usize>],
    cost_tiers: bool,
) -> SimAgent {
    let mut draws = Rng::seeded(mix(draw.seed, index as u64));
    let evals: Vec<(TopicId, i32)> = draw
        .names
        .iter()
        .enumerate()
        .map(|(topic_index, topic)| {
            let base = if topic == draw.truth {
                TRUE_QUALITY
            } else {
                DECOY_QUALITY
            };
            let half_width = match expert_of[topic_index] {
                Some(owner) if owner == index => draw.noise,
                Some(_) => draw.noise.saturating_mul(LAY_NOISE_PERCENT) / 100,
                None => draw.noise,
            };
            (topic.clone(), base + draws.centered(half_width))
        })
        .collect();
    let mut agent = SimAgent::assembled(id, role, draw.seed, index, evals);
    agent.specialty = expert_of
        .iter()
        .position(|holder| *holder == Some(index))
        .and_then(|topic_index| draw.names.get(topic_index).cloned());
    agent.expert_elsewhere = expert_of
        .iter()
        .enumerate()
        .filter_map(|(topic_index, holder)| match holder {
            Some(holder) if *holder != index => draw.names.get(topic_index).cloned(),
            _ => None,
        })
        .collect();
    if cost_tiers && agent.specialty.is_some() {
        agent.cost_unit = SPECIALIST_COST_UNIT;
    }
    agent
}

/// Build one member under `Expertise::HiddenProfile`.
pub(super) fn hidden_profile_agent(
    id: &str,
    role: Role,
    index: usize,
    draw: &MemberDraw<'_>,
    decisive_index: Option<usize>,
    planted_index: Option<usize>,
) -> SimAgent {
    let mut draws = Rng::seeded(mix(draw.seed, index as u64));
    let is_decisive = decisive_index == Some(index);
    let evals: Vec<(TopicId, i32)> = draw
        .names
        .iter()
        .enumerate()
        .map(|(topic_index, topic)| {
            let is_truth = topic == draw.truth;
            let is_planted = planted_index == Some(topic_index);
            let base = if is_truth {
                TRUE_QUALITY
            } else {
                DECOY_QUALITY
            };
            let lift = if is_planted && !is_decisive {
                HIDDEN_LIFT
            } else {
                0
            };
            let half_width = if is_truth && is_decisive {
                draw.noise / EXPERT_NOISE_DIVISOR
            } else {
                draw.noise
            };
            (topic.clone(), base + lift + draws.centered(half_width))
        })
        .collect();
    let mut agent = SimAgent::assembled(id, role, draw.seed, index, evals);
    let planted = planted_index.and_then(|topic_index| draw.names.get(topic_index).cloned());
    // The decisive member is the room's specialist *on the planted decoy* --
    // it is the one member holding a reading of that option nobody else has.
    // Saying so is what gives `!defer` something to fire on: a lay member
    // stuck on the decoy can stand aside for the member who owns it, rather
    // than guessing. It gives nothing away, because the decoy is not the
    // answer.
    if is_decisive {
        agent.refutes.clone_from(&planted);
        agent.specialty = planted;
    } else {
        agent.expert_elsewhere = planted.into_iter().collect();
    }
    agent
}

/// The reproducibility self-check: pins agent 0 of room 0 at seed 1 against
/// golden evaluations, recorded once by running the harness and pasted here,
/// when `TINYHIVEMIND_BENCH_SELFCHECK` is set. A no-op otherwise, and a no-op
/// for any run that is not that exact room.
pub(super) fn selfcheck_uniform(
    expertise: Expertise,
    seed: u64,
    noise: u32,
    topic_count: usize,
    names: &[TopicId],
    members: &[SimAgent],
) {
    if matches!(expertise, Expertise::Uniform)
        && seed == SELFCHECK_ROOM_SEED
        && noise == 90
        && topic_count >= 3
        && std::env::var_os("TINYHIVEMIND_BENCH_SELFCHECK").is_some()
        && let Some(agent0) = members.first()
    {
        // Recorded once and pasted here: agent 0 of room 0 at seed 1, default
        // `--noise 90`, its first three of four evaluations (`stage`,
        // `ship`, `revert`). A change here is a change in the noise draw,
        // not in the weather.
        debug_assert_eq!(agent0.score(&names[0]), -3, "stage eval drifted");
        debug_assert_eq!(agent0.score(&names[1]), -19, "ship eval drifted");
        debug_assert_eq!(agent0.score(&names[2]), 71, "revert eval drifted");
    }
}
