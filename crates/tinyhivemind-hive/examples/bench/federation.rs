//! Several desks, each holding a *correlated* view of the same question.
//!
//! [`crate::sim`] models one room: every member draws independent noise around
//! a shared truth, so the room's average is already unbiased and deliberation
//! only has to find it. That is the right model for one channel and the wrong
//! model for several, because it makes a channel boundary free — any desk
//! could answer alone.
//!
//! A federation adds the thing that makes a boundary cost something. Each desk
//! carries a **bias of its own**: one option it systematically overrates,
//! because everybody on that desk reads the same transcript, works the same
//! part of the system, and is wrong about the same thing. Within a desk that
//! bias is invisible — every member confirms every other — and no amount of
//! within-desk deliberation removes it, because averaging correlated error
//! does not cancel it. Across desks the biases are independent and *do*
//! cancel.
//!
//! So the answer is reachable only by pooling across channels, which is
//! exactly the operation `referral` adds and exactly what a siloed control
//! cannot do. This is the multi-channel form of the hidden profile the live
//! scenarios use, written in numbers so it can be run ten thousand times.

use tinyhivemind_hive::trace::TopicId;

use crate::rng::{Rng, mix};
use crate::sim::{MEMBER_ROLES, SimAgent, TOPIC_NAMES};

/// Names drawn on, in order, for a federation's desks.
const DESK_NAMES: [(&str, &str); 4] = [
    ("payments", "Payments"),
    ("platform", "Platform"),
    ("mobile", "Mobile"),
    ("data", "Data"),
];

/// Evaluation of the genuinely best option, before bias and noise.
const TRUE_QUALITY: i32 = 100;
/// Evaluation of every other option, before bias and noise.
const DECOY_QUALITY: i32 = 40;

/// One desk in a federation, and the option it is collectively wrong about.
#[derive(Clone, Debug)]
pub(crate) struct FederatedDesk {
    /// Canonical desk id, and the conversation the desk's episode runs on.
    pub(crate) id: String,
    /// Operator-facing display name, used in the text agents exchange.
    pub(crate) name: String,
    /// Member ids, in seating order.
    pub(crate) members: Vec<String>,
    /// The option every member of this desk overrates.
    pub(crate) decoy: TopicId,
}

/// Several desks deciding one question, each with a blind spot of its own.
#[derive(Clone, Debug)]
pub(crate) struct Federation {
    /// The option that is genuinely best.
    pub(crate) truth: TopicId,
    /// The options on offer.
    pub(crate) topics: Vec<TopicId>,
    /// The desks, in snapshot order.
    pub(crate) desks: Vec<FederatedDesk>,
    /// Every member, flattened in desk order.
    pub(crate) agents: Vec<SimAgent>,
}

impl Federation {
    /// Generate a reproducible federation.
    ///
    /// `bias` is how much a desk overrates its own decoy. It is the knob that
    /// decides whether the problem is federated at all: at `0` every desk can
    /// answer alone and crossing a channel buys nothing, and above the
    /// 60-point gap between the true option and a decoy the desk's own
    /// average points at the wrong answer and only pooling across desks
    /// recovers it.
    ///
    /// `noise` is the half-width of the *individual* error on top of that,
    /// which is what keeps any one member from being an oracle for its desk.
    pub(crate) fn generate(
        seed: u64,
        desks: usize,
        per_desk: usize,
        topics: usize,
        noise: u32,
        bias: i32,
    ) -> Self {
        let topics = topics.clamp(2, TOPIC_NAMES.len());
        let desk_count = desks.clamp(2, DESK_NAMES.len());
        let per_desk = per_desk.clamp(2, MEMBER_ROLES.len());
        let names: Vec<TopicId> = TOPIC_NAMES
            .iter()
            .take(topics)
            .map(|name| TopicId::from(*name))
            .collect();
        // Placed by the seed rather than at a fixed index, so no arm can score
        // by preferring the first option.
        let truth_index = usize::try_from(mix(seed, 0x7275_7468) % (topics as u64)).unwrap_or(0);
        let truth = names
            .get(truth_index)
            .cloned()
            .unwrap_or_else(|| TopicId::from("stage"));

        // Every desk is wrong about a *different* option. Two desks sharing a
        // decoy would agree with each other for the wrong reason, which is a
        // failure mode worth studying but not the one being measured here.
        let decoys: Vec<TopicId> = (0..desk_count)
            .map(|desk| {
                let others: Vec<&TopicId> = names.iter().filter(|topic| **topic != truth).collect();
                let pick = (usize::try_from(mix(seed, 0xDEC0_1000)).unwrap_or(0) + desk)
                    % others.len().max(1);
                others
                    .get(pick)
                    .map_or_else(|| truth.clone(), |topic| (*topic).clone())
            })
            .collect();

        let mut agents = Vec::new();
        let mut records = Vec::new();
        for (desk, (id, name)) in DESK_NAMES.iter().take(desk_count).enumerate() {
            let decoy = decoys.get(desk).cloned().unwrap_or_else(|| truth.clone());
            let mut members = Vec::new();
            for (seat, (role_name, role)) in MEMBER_ROLES.iter().take(per_desk).enumerate() {
                // Ids are desk-qualified because a federation has more seats
                // than there are role names, and because a transcript that
                // says `platform-critic` reads as what it is.
                let agent_id = format!("{id}-{role_name}");
                let index = desk.saturating_mul(MEMBER_ROLES.len()).saturating_add(seat);
                let mut draws = Rng::seeded(mix(seed, index as u64));
                let evals = names
                    .iter()
                    .map(|topic| {
                        let base = if *topic == truth {
                            TRUE_QUALITY
                        } else {
                            DECOY_QUALITY
                        };
                        let slant = if *topic == decoy { bias } else { 0 };
                        (
                            topic.clone(),
                            base.saturating_add(slant)
                                .saturating_add(draws.centered(noise)),
                        )
                    })
                    .collect();
                agents.push(SimAgent::assembled(&agent_id, *role, seed, index, evals));
                members.push(agent_id);
            }
            records.push(FederatedDesk {
                id: (*id).to_owned(),
                name: (*name).to_owned(),
                members,
                decoy,
            });
        }

        Self {
            truth,
            topics: names,
            desks: records,
            agents,
        }
    }

    /// The desk a member sits on.
    pub(crate) fn desk_of(&self, agent_id: &str) -> Option<&FederatedDesk> {
        self.desks
            .iter()
            .find(|desk| desk.members.iter().any(|member| member == agent_id))
    }

    /// The index of a member in [`Self::agents`].
    pub(crate) fn seat_of(&self, agent_id: &str) -> Option<usize> {
        self.agents.iter().position(|agent| agent.id == agent_id)
    }
}
