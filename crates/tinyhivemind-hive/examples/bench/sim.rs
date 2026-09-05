//! Simulated participants with noisy private evaluations of a shared task.
//!
//! The room decides between `topic_count` options, exactly one of which is
//! genuinely best. Every participant holds a *private* evaluation of each
//! option — the truth plus noise — so no single participant is reliable and
//! the room's only route to the right answer is to pool what its members
//! independently believe. That is the property the benchmark measures: whether
//! a bounded deliberation aggregates noisy private signals better than the
//! single-responder ladder does, at a comparable turn budget.
//!
//! The agents are deliberately mechanical. A language model would make the
//! numbers unreproducible and would confound protocol quality with model
//! quality; `live` drives the same protocol through a real agent CLI when that
//! is what is wanted.
//!
//! # The evidence-first opening
//!
//! Under `--blind-evidence` a member's first turn, while the room is still
//! [`Visibility::Blind`], is a *deposit* rather than a position: it states its
//! own reading of the topic it knows best and nothing else. Proposals begin
//! once the room goes to [`Visibility::Full`].
//!
//! This is a **participant policy, not a library mechanism**. Nothing in
//! `tinyhivemind-hive` can make a member open this way — `step` decides who
//! speaks, never what they say — and the flag exists because the alternative
//! makes every floor mechanism unreachable. A room whose members share a bias
//! puts the same option on the floor four times inside the blind round, which
//! is already four distinct supporters, which is already quorum: the episode's
//! first non-blind turn is a commit turn and an arriving fact has nothing left
//! to change. That is the same finding the live rooms recorded — in every
//! correct episode of
//! `docs/experiments/2026-09-01-live-hidden-profile.md` the five blind turns
//! were five `!evidence` lines, one per member — and the same one
//! `docs/experiments/2026-09-02-federated-hidden-profile.md` reached from the
//! other side, where moving a desk's question to *before* it had backed
//! anything was the difference between the whole federation failing and 77.5%.
//!
//! So the flag is off by default, and every published number that does not ask
//! for it is unchanged. What it buys, and what it costs, is a result rather
//! than an assumption.
//!
//! [`Visibility::Blind`]: tinyhivemind_hive::Visibility::Blind
//! [`Visibility::Full`]: tinyhivemind_hive::Visibility::Full

use std::fmt::Write as _;

use tinyhivemind_hive::{
    HiveTurn, Phase, QuorumPolicy, Sequence, SessionMessage, Visibility,
    quorum::{TopicStanding, standings},
    trace::{TopicId, Trace, TraceKind, resolve},
};

use crate::rng::{Rng, mix};

/// Names drawn on, in order, for a room's options.
pub(crate) const TOPIC_NAMES: [&str; 8] = [
    "stage", "ship", "revert", "shadow", "canary", "freeze", "split", "pilot",
];

/// Names and roles drawn on, in order, for a room's members.
pub(crate) const MEMBER_ROLES: [(&str, Role); 8] = [
    ("planner", Role::Proposer),
    ("critic", Role::Critic),
    ("archivist", Role::Archivist),
    ("scout", Role::Proposer),
    ("auditor", Role::Critic),
    ("historian", Role::Archivist),
    ("builder", Role::Proposer),
    ("reviewer", Role::Critic),
];

/// How a participant fills a turn it has no strong move for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Role {
    /// Puts its own best option on the floor early.
    Proposer,
    /// Objects to a leading option it privately rates poorly.
    Critic,
    /// Supplies grounds without taking a side.
    Archivist,
}

/// Evaluation of the genuinely best option, before noise.
const TRUE_QUALITY: i32 = 100;
/// Evaluation of every other option, before noise.
const DECOY_QUALITY: i32 = 40;
/// Per-mille chance a participant emits prose carrying no marker at all.
const NONCOMPLIANCE: u32 = 60;
/// How much one additional independent backer is worth against a participant's
/// own private evaluation of an option.
///
/// This is the whole reason a room can beat its own members. A participant
/// reads the medium as evidence: three peers who independently put the same
/// option on the floor are informative about that option, and weighing that
/// against a private signal is ordinary belief updating rather than conformity.
/// Set it to zero and the room degenerates into a plurality of first
/// impressions, which is exactly the `vote` control arm.
const SOCIAL_WEIGHT: i32 = 25;
/// The largest refutation cap a real room could ever satisfy.
///
/// A participant does not spend a turn on a move that cannot take effect, so a
/// `None` cap, or one above the largest possible desk, is how the control arms
/// turn refutation off without the simulated members behaving differently in
/// any other way.
const REACHABLE_REFUTATION_CAP: u32 = 8;
const _: () = assert!(MEMBER_ROLES.len() == REACHABLE_REFUTATION_CAP as usize);
/// How far below its own choice a participant will still close a decision out.
///
/// A room whose members each hold out for a private preference nobody else
/// shares does not deadlock — it simply runs out of budget with every option
/// one supporter short. Conceding a near-decision is the move that ends a
/// deliberation, and bounding it is what keeps that from being a cascade: a
/// participant closes on the leader only when the leader is not clearly worse
/// than what it wanted, which is the same 60-point gap that separates the
/// genuinely best option from the rest.
const CONCESSION: i32 = 60;

/// Half-width divisor applied to a topic when the member evaluating it is
/// that topic's own expert, or the hidden-profile member who holds the fact
/// that refutes the planted decoy.
///
/// An expert reads its own topic far more tightly than anyone else -- still
/// noisy, never oracular -- which is what makes pooling its read worth a
/// turn.
const EXPERT_NOISE_DIVISOR: u32 = 6;

/// Percentage of `noise` applied to a topic when somebody *else* is its
/// expert.
///
/// Information about a specialised topic is redistributed rather than
/// created: a lay member's read of somebody else's specialty widens as the
/// expert's narrows, so the room's aggregate uncertainty is unchanged rather
/// than added to.
const LAY_NOISE_PERCENT: u32 = 150;

/// How much the hidden-profile decoy is lifted for everybody except the
/// member holding the fact that refutes it.
///
/// Added to [`DECOY_QUALITY`], so at 100 the planted decoy reads **140**
/// against the true option's **100**: a 40-point lead over the answer, for
/// every member but one.
///
/// Bounded on both sides, the way `--bias` is, and both bounds are what make
/// the problem a hidden profile rather than merely a noisy one.
///
/// *Above* zero by enough that a lay member's own argmax is the decoy however
/// its noise fell. At the `--hidden-profile` noise default of ±50 the
/// difference of two independent draws is triangular on ±100, so a 40-point
/// lead survives it `1 - (60/100)² / 2 ≈ 82%` of the time. Five members
/// polled independently answer the decoy by plurality essentially always,
/// which is what puts the matched-budget vote at zero *by construction* —
/// that is the definition of a hidden profile, not a result.
///
/// *Below* [`GROUNDS_WEIGHT`], so that one grounded refutation, once a member
/// has actually seen it, is enough to put the truth ahead of the decoy on
/// that member's own arithmetic. At the earlier lift of 150 the lead was 90
/// against a discount of 120 only because the discount was set higher still;
/// pinning the lead under a single refutation is what makes the fact
/// *sufficient* rather than merely *relevant*, and it is the difference
/// between a room that can pool and a room whose members can only ever
/// out-vote each other.
const HIDDEN_LIFT: i32 = 100;

/// How much one deposited refuting fact discounts a topic's posterior.
///
/// Distinct from `refutation_cap`, which caps a topic outright: this is the
/// softer discount a member applies on its own account, and it is inert
/// whenever nobody deposits a refuting fact — which is every room outside
/// `Expertise::HiddenProfile`, so no published uniform number moves with it.
///
/// Bounded on both sides, and both bounds are what make the hidden profile
/// solvable-but-not-trivial. A lay member abandons the planted decoy when the
/// decoy's posterior falls under the true option's, and the decoy's lead over
/// the truth for a mean lay member is `HIDDEN_LIFT - 60 = 40` on its own
/// reading plus [`SOCIAL_WEIGHT`] for every peer backing the decoy that is not
/// also backing the truth.
///
/// - *Above* the bare lead of `40`, so the fact **alone** flips a mean lay
///   member that has seen the deposit and has yet to see anybody back
///   anything. A hidden profile whose deciding fact cannot move the member
///   who hears it is not a hidden profile; it is a fact with no route to the
///   decision, which is what the arm was measuring before.
/// - *Below* `40 + 25 = 65`, the lead once one peer is already behind the
///   decoy, so a member reading the fact against a room that has started
///   backing the decoy is **not** flipped by the fact on its own — it takes
///   the fact plus a peer that has already crossed. Two signals carry where
///   one does not, which is the pooling the arm exists to measure.
///
/// At 45 that window is `(40, 65)` and the value sits near its floor, which
/// is the conservative end: the fact is decisive for the first reader and has
/// to be seconded for every reader after that.
const GROUNDS_WEIGHT: i32 = 45;

/// The sentence a deposit carries when it is a *refutation* of the topic it
/// names rather than a *reading* of it.
///
/// The library's grammar has one marker for both: `!evidence #topic` deposits
/// grounds, and a pure fold cannot tell grounds that kill a hypothesis from
/// grounds that merely describe it — that is the finding
/// `docs/experiments/2026-09-01-live-hidden-profile.md` recorded, and the
/// reason `!refute` exists as a separate marker at all. The simulated reader
/// distinguishes them by reading the sentence, which is the modelling
/// shortcut standing in for a participant that understands what the fact
/// says. Nothing in the library depends on it, and the distinction only
/// matters once [`SimAgent::blind_evidence`] has ordinary members depositing
/// readings of their own.
const RULES_OUT: &str = "rules this one out";

/// What a specialist's own turn costs, against a lay member's `1`, when a
/// room is generated with `cost_tiers` set.
///
/// The ratio is the claim this benchmark makes, not the absolute number.
pub(crate) const SPECIALIST_COST_UNIT: u32 = 10;

/// `mix(1, 0)` -- the seed [`Room::generate_with`] receives for room 0 when
/// the whole benchmark is run at its own default `--seed 1`.
/// [`Room::generate_with`]'s reproducibility self-check only ever fires for
/// this one room, so an intentionally different `--seed` run never trips it.
const SELFCHECK_ROOM_SEED: u64 = 13_757_245_211_066_428_519;

/// How private evaluations are distributed across a room.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Expertise {
    /// Every member draws independent noise around the shared truth. Today's
    /// behaviour, unchanged by anything below.
    Uniform,
    /// `count` members each hold one topic far more tightly than everybody
    /// else, and everybody else's read of that topic widens to match.
    Specialists {
        /// Members given a specialty, clamped to the room and to the topics
        /// on offer.
        count: usize,
    },
    /// One decoy is planted above every member's own argmax except one, who
    /// alone holds the fact that rules it out.
    HiddenProfile,
}

/// One simulated room: the options, which is best, and who is in it.
#[derive(Clone, Debug)]
pub(crate) struct Room {
    /// The option that is genuinely best.
    pub(crate) truth: TopicId,
    /// The participants.
    pub(crate) agents: Vec<SimAgent>,
    /// The member holding each specialised topic, one entry per topic that
    /// has an expert. Populated under `Expertise::Specialists` only; empty
    /// under `Expertise::Uniform`, which has no experts, and under
    /// `Expertise::HiddenProfile`, whose one knowledgeable member is named by
    /// `decisive` instead.
    ///
    /// Scoring data only: nothing a participant reads may consult this list
    /// directly. A member learns of its own specialty and of anyone else's
    /// only through `SimAgent::specialty` and `SimAgent::expert_elsewhere` —
    /// built from this list for a room of specialists, and from the planted
    /// decoy for a hidden profile, whose fact-holder is the room's specialist
    /// on the option it alone can rule out.
    pub(crate) experts: Vec<(TopicId, String)>,
    /// The member holding the fact that refutes the hidden-profile decoy,
    /// under `Expertise::HiddenProfile`. `None` under every other shape.
    ///
    /// Scoring data only, for the same reason `experts` is.
    pub(crate) decisive: Option<String>,
    /// The topic planted as the hidden-profile decoy, under
    /// `Expertise::HiddenProfile`. `None` under every other shape.
    ///
    /// Scoring data only.
    pub(crate) planted: Option<TopicId>,
}

impl Room {
    /// Generate a reproducible room.
    ///
    /// `noise` is the half-width of the uniform error on each private
    /// evaluation: at `0` every member already knows the answer, and as it
    /// approaches the 60-point quality gap the members become individually
    /// unreliable while remaining collectively informative.
    pub(crate) fn generate(seed: u64, agents: usize, topics: usize, noise: u32) -> Self {
        Self::generate_with(seed, agents, topics, noise, Expertise::Uniform, false)
    }

    /// Generate a reproducible room under one of the [`Expertise`] shapes.
    ///
    /// `cost_tiers` charges a specialist [`SPECIALIST_COST_UNIT`] where a lay
    /// member costs `1`; both the vote arm and a deliberation's own
    /// `cost_units` read the charge off [`SimAgent::cost_unit`].
    ///
    /// `Expertise::Uniform` never touches the separate expertise stream this
    /// draws its specialists and its hidden profile from, so a room
    /// generated under it -- and therefore every published number that does
    /// not opt into a different shape -- is unchanged by anything this
    /// function adds. The per-member draw order [`SimAgent::new`] uses is
    /// untouched for the same reason: this function calls it, unchanged, for
    /// that one shape.
    pub(crate) fn generate_with(
        seed: u64,
        agents: usize,
        topics: usize,
        noise: u32,
        expertise: Expertise,
        cost_tiers: bool,
    ) -> Self {
        let topic_count = topics.clamp(2, TOPIC_NAMES.len());
        let agent_count = agents.clamp(2, MEMBER_ROLES.len());
        let names: Vec<TopicId> = TOPIC_NAMES
            .iter()
            .take(topic_count)
            .map(|name| TopicId::from(*name))
            .collect();
        // The truth is placed by the seed rather than at a fixed index, so no
        // arm of the benchmark can score by preferring the first option.
        let truth_index =
            usize::try_from(mix(seed, 0x7275_7468) % (topic_count as u64)).unwrap_or(0);
        let truth = names
            .get(truth_index)
            .cloned()
            .unwrap_or(TopicId::from("stage"));

        // A stream of its own, seeded independently of every per-member
        // draw: under `Expertise::Uniform` it is built and never asked for a
        // value, so the room it produces is bit-identical to one generated
        // before this function existed.
        let mut expert_rng = Rng::seeded(mix(seed, 0x6578_7072));
        let (expert_of, decisive_index, planted_index) = draw_expertise(
            &mut expert_rng,
            agent_count,
            topic_count,
            truth_index,
            expertise,
        );

        let draw = MemberDraw {
            seed,
            names: &names,
            truth: &truth,
            noise,
        };
        let members: Vec<SimAgent> = MEMBER_ROLES
            .iter()
            .take(agent_count)
            .enumerate()
            .map(|(index, (id, role))| match expertise {
                Expertise::Uniform => SimAgent::new(id, *role, seed, index, &names, &truth, noise),
                Expertise::Specialists { .. } => {
                    specialist_agent(id, *role, index, &draw, &expert_of, cost_tiers)
                }
                Expertise::HiddenProfile => {
                    hidden_profile_agent(id, *role, index, &draw, decisive_index, planted_index)
                }
            })
            .collect();

        selfcheck_uniform(expertise, seed, noise, topic_count, &names, &members);

        let experts: Vec<(TopicId, String)> = expert_of
            .iter()
            .enumerate()
            .filter_map(|(topic_index, holder)| {
                let member = (*holder)?;
                let topic = names.get(topic_index)?.clone();
                let id = members.get(member)?.id.clone();
                Some((topic, id))
            })
            .collect();
        let decisive =
            decisive_index.and_then(|index| members.get(index).map(|agent| agent.id.clone()));
        let planted = planted_index.and_then(|index| names.get(index).cloned());

        Self {
            truth,
            agents: members,
            experts,
            decisive,
            planted,
        }
    }

    /// The member ids, in desk order.
    pub(crate) fn member_ids(&self) -> Vec<&str> {
        self.agents.iter().map(|agent| agent.id.as_str()).collect()
    }

    /// The member whose knowledge the room's answer actually turns on: the
    /// hidden profile's decisive fact-holder, or the specialist on the
    /// genuinely best option. `None` for a uniform room, which has neither.
    ///
    /// Scoring data, read only after an arm has already decided: it is how
    /// `route %` learns whether the responder ladder picked the member who
    /// actually held the deciding topic, and it is the same member
    /// `run_episode` scores `fact %` against. Nothing a participant reads
    /// may consult it.
    pub(crate) fn deciding_expert(&self) -> Option<&str> {
        if let Some(decisive) = &self.decisive {
            return Some(decisive.as_str());
        }
        self.experts
            .iter()
            .find(|(held, _)| *held == self.truth)
            .map(|(_, id)| id.as_str())
    }

    /// The same room with every member's turn charged at `unit`.
    ///
    /// The `all-reasoning` control: a room that spends the expensive tier on
    /// every seat rather than only on the members that need it. Nothing else
    /// about the room moves, so the two arms differ in price and in nothing
    /// else.
    pub(crate) fn at_cost(&self, unit: u32) -> Self {
        let mut room = self.clone();
        for agent in &mut room.agents {
            agent.cost_unit = unit;
        }
        room
    }

    /// The same room, with every member opening on a deposit rather than a
    /// position.
    ///
    /// Applied once, to the generated room, so every arm that deliberates it
    /// — and every replay [`Room::resampled`] makes of it — sees the same
    /// participants. The control arms read nothing but
    /// [`SimAgent::favourite`], which this does not touch, so `vote` and
    /// `ladder` are unaffected by construction.
    pub(crate) fn set_blind_evidence(&mut self, on: bool) {
        for agent in &mut self.agents {
            agent.set_blind_evidence(on);
        }
    }

    /// The same room, same private evaluations, with every member's
    /// noncompliance draw reseeded.
    ///
    /// `--history` needs several *different* episodes of one room to earn a
    /// directory from. The simulated participants are otherwise fully
    /// deterministic given the room, so replaying it would produce the same
    /// transcript N times and a directory N times as heavy but no better
    /// informed. Reseeding the one stream that is genuinely a sample — which
    /// turns come out as prose rather than as a marker — resamples the
    /// transcript without touching a single private evaluation.
    pub(crate) fn resampled(&self, seed: u64) -> Self {
        let mut room = self.clone();
        for (index, agent) in room.agents.iter_mut().enumerate() {
            agent.rng = Rng::seeded(mix(seed, 0x000A_11CE ^ index as u64));
        }
        room
    }

    /// What one member's own turn costs, by id.
    ///
    /// A convenience for a caller that only holds an id and not a
    /// [`Participant`](crate::run::Participant) reference -- the vote arm
    /// charges its matched budget this way, one member at a time, without
    /// building a trait object for each. `1` for a member `Room::generate_with`
    /// did not find, the same default every member starts at.
    pub(crate) fn cost_of(&self, agent_id: &str) -> u32 {
        self.agents
            .iter()
            .find(|agent| agent.id == agent_id)
            .map_or(1, |agent| agent.cost_unit)
    }
}

/// Draw the per-topic expert assignment and, for a hidden profile, its
/// decisive member and its planted decoy, from a stream seeded independently
/// of every per-member draw. Under `Expertise::Uniform` this never asks the
/// stream for a value at all.
fn draw_expertise(
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
struct MemberDraw<'a> {
    seed: u64,
    names: &'a [TopicId],
    truth: &'a TopicId,
    noise: u32,
}

/// Build one member under `Expertise::Specialists`.
fn specialist_agent(
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

/// Build one member under `Expertise::HiddenProfile`.
fn hidden_profile_agent(
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
fn selfcheck_uniform(
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

/// A participant that holds a private, noisy view of every option.
#[derive(Clone, Debug)]
pub(crate) struct SimAgent {
    /// Canonical agent id.
    pub(crate) id: String,
    /// How it fills a turn with no strong move.
    pub(crate) role: Role,
    /// Private evaluation per topic, aligned with [`Room::topics`].
    evals: Vec<(TopicId, i32)>,
    /// Readings of a topic that arrived from outside this member's own desk,
    /// as a running sum and a count.
    ///
    /// A member that hears another channel's reading of an option does not
    /// replace its own with it and does not simply defer to it: it averages
    /// the two, which is the whole operation a channel boundary otherwise
    /// prevents. Nothing here touches the supporter sets — an imported reading
    /// changes what a member *believes*, and it still has to spend a turn
    /// saying so before the room counts it.
    imports: Vec<(TopicId, i64, u32)>,
    /// Its own argmax over `evals`.
    favourite: TopicId,
    /// Drives noncompliance only; never the private evaluations.
    rng: Rng,
    /// The room's quorum rule, which is public: a participant is entitled to
    /// know how many grounded supporters settle a question, and reads the
    /// medium through the library's own fold rather than a private imitation
    /// of it.
    quorum: QuorumPolicy,
    /// The topic this member specialises in, under `Expertise::Specialists`.
    /// `None` under every other shape.
    specialty: Option<TopicId>,
    /// The topic this member holds a refuting fact for, under
    /// `Expertise::HiddenProfile`. `None` for every other member and shape.
    refutes: Option<TopicId>,
    /// Topics some *other* member specialises in. Never contains this
    /// member's own `specialty`.
    expert_elsewhere: Vec<TopicId>,
    /// What this member's own turn costs, charged by the vote arm and summed
    /// into a deliberation's `cost_units`. `1` unless `Room::generate_with`
    /// was asked for `cost_tiers` and this member is a specialist.
    cost_unit: u32,
    /// Turns this member may defer instead of arguing outside its specialty.
    /// `0` turns the move off.
    defer_cap: u32,
    /// Turns this member has already deferred.
    deferred: u32,
    /// Whether this member opens with a deposit rather than a position, and
    /// puts its own best option on the floor rather than backing a worse one
    /// somebody else got there first with. See the module docs: it is a
    /// participant policy, off unless `--blind-evidence` asked for it.
    blind_evidence: bool,
}

impl SimAgent {
    fn new(
        id: &str,
        role: Role,
        seed: u64,
        index: usize,
        topics: &[TopicId],
        truth: &TopicId,
        noise: u32,
    ) -> Self {
        let mut draws = Rng::seeded(mix(seed, index as u64));
        let evals: Vec<(TopicId, i32)> = topics
            .iter()
            .map(|topic| {
                let base = if topic == truth {
                    TRUE_QUALITY
                } else {
                    DECOY_QUALITY
                };
                (topic.clone(), base + draws.centered(noise))
            })
            .collect();
        Self::assembled(id, role, seed, index, evals)
    }

    /// Build a participant from evaluations the caller has already drawn.
    ///
    /// [`Self::new`] draws independent noise around a shared truth, which is
    /// the right model for one room. A federation of desks needs evaluations
    /// whose error is *correlated within a desk*, so it draws its own and
    /// hands them in here.
    pub(crate) fn assembled(
        id: &str,
        role: Role,
        seed: u64,
        index: usize,
        evals: Vec<(TopicId, i32)>,
    ) -> Self {
        let favourite = evals
            .iter()
            .max_by_key(|(_, score)| *score)
            .map_or_else(|| TopicId::from("stage"), |(topic, _)| topic.clone());
        Self {
            id: id.to_owned(),
            role,
            evals,
            imports: Vec::new(),
            favourite,
            rng: Rng::seeded(mix(seed, 0x000A_11CE ^ index as u64)),
            quorum: QuorumPolicy::DEFAULT,
            specialty: None,
            refutes: None,
            expert_elsewhere: Vec::new(),
            cost_unit: 1,
            defer_cap: 0,
            deferred: 0,
            blind_evidence: false,
        }
    }

    /// Open with a deposit rather than a position, under `--blind-evidence`.
    ///
    /// Every room leaves this off, so a run that does not ask for the flag is
    /// bit-identical to one built before the move existed.
    pub(crate) fn set_blind_evidence(&mut self, on: bool) {
        self.blind_evidence = on;
    }

    /// Fold one outside reading of a topic into this member's own view.
    ///
    /// Returns whether the reading was taken: a topic this member holds no
    /// evaluation of is not a topic it can average anything into.
    pub(crate) fn import(&mut self, topic: &TopicId, reading: i32) -> bool {
        if !self.evals.iter().any(|(held, _)| held == topic) {
            return false;
        }
        match self.imports.iter_mut().find(|(held, _, _)| held == topic) {
            Some(entry) => {
                entry.1 = entry.1.saturating_add(i64::from(reading));
                entry.2 = entry.2.saturating_add(1);
            }
            None => self.imports.push((topic.clone(), i64::from(reading), 1)),
        }
        self.favourite = self
            .evals
            .iter()
            .map(|(topic, _)| topic.clone())
            .max_by_key(|topic| self.score(topic))
            .unwrap_or_else(|| self.favourite.clone());
        true
    }

    /// Tell the participant which quorum rule the room is running.
    pub(crate) fn set_quorum(&mut self, quorum: QuorumPolicy) {
        self.quorum = quorum;
    }

    /// Tell the participant how many turns it may spend deferring to a
    /// topic's expert instead of arguing outside its own specialty. `0`
    /// turns the move off.
    ///
    /// `Room::generate_with` leaves every member at `0`; the deferring arms
    /// set a real cap on their own copy of the room through
    /// [`run_episode_with`](crate::run::run_episode_with), so an arm that
    /// does not defer is bit-identical to one built before the move existed.
    pub(crate) fn set_defer_cap(&mut self, cap: u32) {
        self.defer_cap = cap;
        self.deferred = 0;
    }

    /// The option this participant would pick with no deliberation at all.
    ///
    /// This is what the single-responder arms of the benchmark get: one
    /// member's unaided judgement.
    pub(crate) fn favourite(&self) -> &TopicId {
        &self.favourite
    }

    /// This participant's score for one option: its own reading, averaged
    /// with every outside reading it has taken.
    pub(crate) fn score(&self, topic: &TopicId) -> i32 {
        let Some((_, own)) = self.evals.iter().find(|(held, _)| held == topic) else {
            return i32::MIN;
        };
        let Some((_, sum, count)) = self.imports.iter().find(|(held, _, _)| held == topic) else {
            return *own;
        };
        let total = i64::from(*own).saturating_add(*sum);
        let divisor = i64::from(*count).saturating_add(1);
        i32::try_from(total / divisor).unwrap_or(*own)
    }

    /// Produce the body of one turn, seeing exactly what the turn authorized.
    fn compose(&mut self, turn: &HiveTurn, visible: &[&SessionMessage]) -> String {
        let view = View::fold(visible, self.quorum);

        // Real participants do not speak the grammar on every turn. Modelling
        // that is the difference between benchmarking the protocol and
        // benchmarking a formatter.
        if self.rng.chance(NONCOMPLIANCE) {
            return format!(
                "Thinking about this; {} still looks strongest to me.",
                self.favourite
            );
        }

        // The evidence-first opening: while nobody can read anybody, say what
        // you know rather than what you want. This is the whole of
        // `--blind-evidence` on the writing side, and the module docs say why
        // it is a participant policy rather than something the library could
        // impose.
        if self.blind_evidence
            && turn.visibility == Visibility::Blind
            && let Some(line) = self.opening_deposit(&view)
        {
            return line;
        }

        // A hypothesis this member rates *clearly* below its own — the same
        // 60-point gap that separates the genuinely best option from a decoy —
        // is not a tie to be broken but a claim to be killed. Objecting would
        // cost one turn per advocate and grow with every new supporter;
        // refuting costs one turn and caps the topic for the whole room. The
        // gap is what separates the two moves: a merely weaker contender still
        // gets an objection, below.
        if self
            .quorum
            .refutation_cap
            .is_some_and(|cap| cap <= REACHABLE_REFUTATION_CAP)
            && let Some((topic, grounds)) = view.refutable(self)
        {
            return format!("!refute #{topic} ^{grounds} The grounds I hold rule this one out.");
        }

        // This member holds the one fact that rules a hidden-profile decoy
        // out, and it is on the floor: deposit it. Unlike `!refute`, above,
        // this never caps the topic outright -- it only discounts it, in
        // `View::posterior`, for every member who reads the deposit -- so it
        // is available whether or not the room's policy ever turns
        // `refutation_cap` on. Depositing it twice would spend a turn saying
        // nothing new.
        if let Some(topic) = self.refutes.clone()
            && let Some(proposal) = view.proposal(&topic)
            && !view.has_deposited(&self.id, &topic)
        {
            return format!("!evidence #{topic} ^{proposal} The reading I hold {RULES_OUT}.");
        }

        // Two options are both carrying, which is the one state no amount of
        // further support can resolve: a room does not settle by adding weight
        // to one side, because both sides stay above the threshold. Cross-
        // inhibition is the mechanism the library provides for exactly this —
        // object to a *message*, which silences its author as an advocate of
        // whatever it advocated there. That is why the objection is checked
        // before the commit: a member that records a decision into a tie
        // spends the budget without ever reaching one.
        if let Some((topic, target, grounds)) = view.weaker_contender(self) {
            return format!(
                "!object >{target} ^{grounds} I rate {topic} below the other option carrying here."
            );
        }

        // A commit turn records what the room actually carried, not what this
        // member would have preferred.
        if turn.phase == Phase::Commit
            && let Some((topic, grounds)) = view.leading(self)
        {
            return format!("!commit #{topic} ^{grounds} Recording the decision the room reached.");
        }

        // Nothing on the floor is worth as much as something this member is
        // still holding, so put that up instead of backing a worse option.
        //
        // Only under the evidence-first opening, and only because of it: with
        // the ordinary opening every member's favourite reaches the floor in
        // the blind round, so there is never anything better left in a hand.
        // With the floor empty for a whole round, a member that could only
        // ever back what was already there would hand the decision to whoever
        // happened to propose first, and the reading it deposited would
        // decide nothing.
        if self.blind_evidence
            && let Some(topic) = view.better_than_floor(self)
        {
            // Deliberately uncited. A proposal is a conclusion, and citing the
            // deposit it happens to agree with would earn its author directory
            // weight on the topic for the act of arguing it -- which is
            // precisely the circularity the directory exists to avoid, and
            // which measurably kills `BidReason::Knows`: it fires in four
            // episodes in five with the proposal uncited and in fewer than one
            // in ten with it cited. The grounds go on the *support* instead,
            // below, where a member is answering something already said.
            return format!(
                "!propose #{topic} It rates highest once I weigh what the room has stated \
                 against my own read."
            );
        }

        // Back the best option currently on the floor, weighing this member's
        // own signal against how many peers independently backed it. This is
        // the step that pools information across the room.
        if let Some((topic, grounds)) = view.best_proposal(self)
            && !view.has_backed(&self.id, topic)
        {
            // Under the evidence-first opening the grounds are the deposit
            // the room actually stated about this option, where there is one,
            // rather than the proposal restating a preference. That is the
            // only thing in this file that puts a stated fact at the end of a
            // citation chain, which is exactly what `require_evidential` asks
            // a support to have.
            let grounds = if self.blind_evidence {
                view.grounds_for(&self.id, topic).unwrap_or(grounds)
            } else {
                grounds
            };
            let mut line = String::new();
            let _ = write!(
                line,
                "!support #{topic} ^{grounds} It scores highest once I weigh the room against my own read."
            );
            return line;
        }

        // The room is one supporter short of settling and this member has not
        // backed the option in front. Closing that out is what ends a
        // deliberation; holding out for a preference the room does not share
        // is what spends the budget without deciding anything.
        if let Some((topic, grounds)) = view.closable(self) {
            return format!(
                "!support #{topic} ^{grounds} Close enough to my own read to settle it here."
            );
        }

        // A topic outside this member's own specialty is contested, and
        // somebody else on the room owns it: yield the turn to them rather
        // than arguing a read that is not this member's strong suit. The cap
        // keeps a deferring member from stalling the room forever; `0` turns
        // the whole move off, which is every room today until an episode
        // policy grows a field `Room::generate_with` can wire a real cap
        // from.
        if self.defer_cap > self.deferred
            && let Some(topic) = view.deferrable(self)
        {
            self.deferred = self.deferred.saturating_add(1);
            return format!(
                "!defer #{topic} Not my area — somebody who owns this should weigh in."
            );
        }

        // Nothing worth backing is on the floor, so put an option there.
        if view.proposal(&self.favourite).is_none() {
            return format!(
                "!propose #{} It is the option I rate highest.",
                self.favourite
            );
        }

        match self.role {
            // A critic keeps pressing on an option it privately rates poorly
            // even when the room is not yet tied.
            Role::Critic => {
                if let Some((topic, target)) = view.rival_advocacy(self)
                    && let Some(grounds) = view.proposal(&self.favourite)
                {
                    return format!(
                        "!object >{target} ^{grounds} I rate {topic} below the alternative on the floor."
                    );
                }
                self.evidence(&view)
            }
            Role::Proposer | Role::Archivist => self.evidence(&view),
        }
    }

    /// The deposit this member opens the room with under `--blind-evidence`:
    /// its own reading of the one topic it knows best, stated before anybody
    /// has taken a position.
    ///
    /// The topic is the fact this member holds against a planted decoy where
    /// it holds one, its specialty where it has one, and otherwise its own
    /// favourite — "what I know best" in each of the three shapes the
    /// benchmark generates. There is no citation, because nothing is visible
    /// to cite: an uncited `!evidence` is still a full-weight deposit in the
    /// directory, which is what gives `BidReason::Knows` something to route
    /// on later.
    ///
    /// `None` once this member has already deposited on that topic, which
    /// cannot happen inside a blind round that ends when every member has
    /// spoken once, and is checked anyway so the move stays idempotent.
    fn opening_deposit(&self, view: &View) -> Option<String> {
        let topic = self
            .refutes
            .clone()
            .or_else(|| self.specialty.clone())
            .unwrap_or_else(|| self.favourite.clone());
        if view.has_deposited(&self.id, &topic) {
            return None;
        }
        let reading = self.score(&topic);
        // A reading and a refutation are the same marker to the library's
        // fold; the sentence is what separates them for a reader. See
        // `RULES_OUT`.
        if self.refutes.as_ref() == Some(&topic) {
            Some(format!(
                "!evidence #{topic} My own read of it is {reading}. The reading I hold {RULES_OUT}."
            ))
        } else {
            Some(format!(
                "!evidence #{topic} My own read of it is {reading}."
            ))
        }
    }

    /// Add grounds without taking a side.
    fn evidence(&self, view: &View) -> String {
        view.proposal(&self.favourite).map_or_else(
            || "!question What would make one of these options clearly safer?".to_owned(),
            |grounds| {
                format!("!evidence ^{grounds} Prior rollouts of this shape behaved the same way.")
            },
        )
    }
}

/// What one turn can actually see, folded once through the library's own reads.
///
/// The participant does not reimplement the fold. It calls [`resolve`] to read
/// the traces out of the messages it was shown and [`standings`] to see who is
/// still counted as a supporter after cross-inhibition — the same two functions
/// the episode uses. A participant that guessed at the standings instead would
/// be benchmarking the guess.
struct View {
    traces: Vec<Trace>,
    standings: Vec<TopicStanding>,
    threshold: usize,
}

impl View {
    fn fold(visible: &[&SessionMessage], quorum: QuorumPolicy) -> Self {
        let mut traces: Vec<Trace> = visible
            .iter()
            .flat_map(|message| resolve(&message.content, None, &message.author, message.sequence))
            .collect();
        traces.sort_by_key(|trace| (trace.sequence, trace.offset));
        let at = visible
            .last()
            .map_or(Sequence(0), |message| message.sequence);
        let standings = standings(&traces, at, &quorum).unwrap_or_default();
        Self {
            traces,
            standings,
            threshold: usize::try_from(quorum.threshold).unwrap_or(2),
        }
    }

    /// The sequence that first proposed a topic, if it is on the floor.
    fn proposal(&self, topic: &TopicId) -> Option<Sequence> {
        self.traces
            .iter()
            .find(|trace| trace.kind == TraceKind::Propose && trace.topic.as_ref() == Some(topic))
            .map(|trace| trace.sequence)
    }

    /// Every topic on the floor, with the sequence that put it there.
    fn floor(&self) -> Vec<(&TopicId, Sequence)> {
        let mut floor: Vec<(&TopicId, Sequence)> = Vec::new();
        for trace in &self.traces {
            if trace.kind != TraceKind::Propose {
                continue;
            }
            let Some(topic) = trace.topic.as_ref() else {
                continue;
            };
            if !floor.iter().any(|(held, _)| *held == topic) {
                floor.push((topic, trace.sequence));
            }
        }
        floor
    }

    /// The supporters the library still counts for a topic.
    fn backing(&self, topic: &TopicId) -> &[String] {
        self.standings
            .iter()
            .find(|standing| &standing.topic == topic)
            .map_or(&[][..], |standing| &standing.supporters)
    }

    /// How many distinct agents the library still counts for a topic.
    fn backers(&self, topic: &TopicId) -> usize {
        self.backing(topic).len()
    }

    /// A participant's private score for an option, updated by how many *other*
    /// members independently back it, and discounted by how many *other*
    /// members have deposited grounds against it.
    fn posterior(&self, agent: &SimAgent, topic: &TopicId) -> i32 {
        let peers = self
            .backing(topic)
            .iter()
            .filter(|backer| **backer != agent.id)
            .count();
        let peers = i32::try_from(peers).unwrap_or(0);
        let refuters = self.refuters_of(agent, topic);
        agent
            .score(topic)
            .saturating_add(peers.saturating_mul(SOCIAL_WEIGHT))
            .saturating_sub(refuters.saturating_mul(GROUNDS_WEIGHT))
    }

    /// Whether the library still counts this agent as backing a topic.
    fn has_backed(&self, agent_id: &str, topic: &TopicId) -> bool {
        self.backing(topic).iter().any(|held| held == agent_id)
    }

    /// Whether this agent has already deposited an `!evidence` trace naming
    /// `topic`.
    ///
    /// Depositing the same grounds twice would spend a turn saying nothing
    /// new, so a hidden-profile member checks this before repeating its one
    /// fact.
    fn has_deposited(&self, agent_id: &str, topic: &TopicId) -> bool {
        self.traces.iter().any(|trace| {
            trace.kind == TraceKind::Evidence
                && trace.topic.as_ref() == Some(topic)
                && trace.agent_id() == Some(agent_id)
        })
    }

    /// Distinct members who have deposited an `!evidence` trace naming
    /// `topic` **that argues against it**.
    ///
    /// A deposit is read as a refutation when its sentence says so, and as a
    /// reading of the option otherwise — see [`RULES_OUT`] for why a
    /// participant can draw that line and the library's fold cannot. Without
    /// the distinction the evidence-first opening would be self-defeating:
    /// four lay members each stating what they read the planted decoy at
    /// would each be counted as having refuted it, and the decoy would
    /// collapse under the weight of its own supporters' agreement.
    ///
    /// Under `Expertise::Uniform` nobody holds a `refutes` topic, so no
    /// participant ever deposits a refutation and this stays zero for every
    /// topic.
    ///
    /// Unlike the peer count above, this deliberately does *not* exclude
    /// `agent` itself. A peer's backing is social evidence and a member
    /// cannot be its own peer; a stated fact is not social, and a member that
    /// applied everybody's refuting facts except the one it holds itself
    /// would be arguing against its own knowledge. The hidden profile's
    /// fact-holder is exactly that member, and without this it is swamped by
    /// the social weight of the four peers backing the decoy it just refuted.
    fn refuters_of(&self, _agent: &SimAgent, topic: &TopicId) -> i32 {
        let mut seen: Vec<&str> = Vec::new();
        for trace in &self.traces {
            if trace.kind != TraceKind::Evidence || trace.topic.as_ref() != Some(topic) {
                continue;
            }
            if !trace.text.contains(RULES_OUT) {
                continue;
            }
            let Some(author) = trace.agent_id() else {
                continue;
            };
            if seen.contains(&author) {
                continue;
            }
            seen.push(author);
        }
        i32::try_from(seen.len()).unwrap_or(0)
    }

    /// The topic on the floor this member would rather somebody else spoke
    /// to: one it knows another member owns, and the one the room is leaning
    /// hardest on.
    ///
    /// Deliberately *not* restricted to a topic that has yet to carry. A
    /// hidden profile's whole shape is that the option the room has already
    /// backed is the one nobody has the deciding reading of, so a member that
    /// may only stand aside on options nobody is winning with can never stand
    /// aside on the one that matters. Ties break by the order `standings`
    /// returns, which is stable.
    fn deferrable(&self, agent: &SimAgent) -> Option<&TopicId> {
        self.standings
            .iter()
            .map(|standing| &standing.topic)
            .filter(|topic| agent.specialty.as_ref() != Some(*topic))
            .filter(|topic| agent.expert_elsewhere.contains(topic))
            .max_by_key(|topic| self.backers(topic))
    }

    /// The grounds this member would cite for a position on `topic`: its own
    /// deposit about it where it has one, and otherwise the first deposit
    /// anybody made about it.
    ///
    /// Its own first, because a member arguing from what it stated itself is
    /// the ordinary case; anybody's second, because a member persuaded by
    /// somebody else's fact is citing that fact, which is the whole of what a
    /// citation is for.
    fn grounds_for(&self, agent_id: &str, topic: &TopicId) -> Option<Sequence> {
        let deposits = || {
            self.traces.iter().filter(move |trace| {
                trace.kind == TraceKind::Evidence && trace.topic.as_ref() == Some(topic)
            })
        };
        deposits()
            .find(|trace| trace.agent_id() != Some(agent_id))
            .map(|trace| trace.sequence)
    }

    /// The option this member now rates highest over *everything it holds*,
    /// when that option is not on the floor and nothing on the floor scores as
    /// well.
    ///
    /// The comparison is over posteriors on both sides, so a peer's backing
    /// and a deposited refutation both count: a member does not put an option
    /// up merely because it likes it, only because the room has offered it
    /// nothing better. Ties go to the floor, which is what keeps this from
    /// re-opening a question the room has already converged on.
    fn better_than_floor<'a>(&self, agent: &'a SimAgent) -> Option<&'a TopicId> {
        let best = agent
            .evals
            .iter()
            .map(|(topic, _)| topic)
            .max_by_key(|topic| self.posterior(agent, topic))?;
        if self.proposal(best).is_some() {
            return None;
        }
        let held = self
            .floor()
            .into_iter()
            .map(|(topic, _)| self.posterior(agent, topic))
            .max();
        match held {
            Some(held) if held >= self.posterior(agent, best) => None,
            _ => Some(best),
        }
    }

    /// The proposed option this participant rates highest once the room's own
    /// independent backing is weighed against its private evaluation.
    fn best_proposal(&self, agent: &SimAgent) -> Option<(&TopicId, Sequence)> {
        self.floor()
            .into_iter()
            .max_by_key(|(topic, _)| self.posterior(agent, topic))
    }

    /// The topic carrying the most support, breaking ties by this
    /// participant's own updated evaluation, with a sequence to cite.
    fn leading(&self, agent: &SimAgent) -> Option<(&TopicId, Sequence)> {
        self.floor()
            .into_iter()
            .max_by_key(|(topic, _)| (self.backers(topic), self.posterior(agent, topic)))
    }

    /// The option one supporter short of quorum that this participant has not
    /// backed, when it is not clearly worse than what this participant did
    /// back, with grounds to cite.
    fn closable(&self, agent: &SimAgent) -> Option<(&TopicId, Sequence)> {
        let short = self.threshold.checked_sub(1)?;
        let (topic, grounds) = self
            .floor()
            .into_iter()
            .filter(|(topic, _)| self.backers(topic) == short)
            .filter(|(topic, _)| !self.has_backed(&agent.id, topic))
            .max_by_key(|(topic, _)| self.posterior(agent, topic))?;
        let held = self
            .floor()
            .into_iter()
            .filter(|(held, _)| self.has_backed(&agent.id, held))
            .map(|(held, _)| self.posterior(agent, held))
            .max()
            .unwrap_or(i32::MIN);
        if held != i32::MIN && self.posterior(agent, topic) < held.saturating_sub(CONCESSION) {
            return None;
        }
        Some((topic, grounds))
    }

    /// Two or more options carrying at once, and this participant rates one of
    /// them below another: the message to object to, and the grounds to cite.
    ///
    /// "Carrying" is read the way the library reads it — at or above the quorum
    /// threshold, after cross-inhibition — because that is exactly the
    /// condition that deadlocks an episode. Adding support cannot resolve it:
    /// both options stay above the threshold no matter how much weight one
    /// gains. Silencing an advocate can, and that asymmetry is why the
    /// objection targets a message rather than a topic.
    fn weaker_contender(&self, agent: &SimAgent) -> Option<(&TopicId, Sequence, Sequence)> {
        let mut contenders: Vec<&TopicId> = self
            .standings
            .iter()
            .filter(|standing| standing.supporters.len() >= self.threshold)
            .map(|standing| &standing.topic)
            .collect();
        if contenders.len() < 2 {
            return None;
        }
        contenders.sort_by_key(|topic| std::cmp::Reverse(self.posterior(agent, topic)));
        let best = *contenders.first()?;
        let worst = *contenders.last()?;
        if self.posterior(agent, worst) >= self.posterior(agent, best) {
            return None;
        }
        // Silence one advocate of the weaker option: somebody other than this
        // participant, who is still counted as advocating it.
        let target = self.traces.iter().find(|trace| {
            matches!(trace.kind, TraceKind::Propose | TraceKind::Support)
                && trace.topic.as_ref() == Some(worst)
                && trace
                    .agent_id()
                    .is_some_and(|id| id != agent.id && self.has_backed(id, worst))
        })?;
        Some((worst, target.sequence, self.proposal(best)?))
    }

    /// A topic on the floor this participant rates clearly below its own best,
    /// and has not already refuted: the topic, and the grounds to cite.
    ///
    /// The threshold is [`CONCESSION`], the same gap that separates the true
    /// option from a decoy, so a refutation is spent on a hypothesis this
    /// member believes is wrong rather than on one it merely likes less. Ties
    /// between two plausible options stay the objection's business.
    ///
    /// Grounds are this member's own evidence where it has deposited any, and
    /// otherwise the proposal being argued against.
    fn refutable(&self, agent: &SimAgent) -> Option<(&TopicId, Sequence)> {
        let mine = agent.score(&agent.favourite);
        let topic = self
            .standings
            .iter()
            .filter(|standing| standing.topic != agent.favourite)
            .filter(|standing| !standing.refuted_by.contains(&agent.id))
            .filter(|standing| mine.saturating_sub(agent.score(&standing.topic)) > CONCESSION)
            .max_by_key(|standing| standing.supporters.len())
            .map(|standing| &standing.topic)?;
        let own_evidence = self.traces.iter().find(|trace| {
            trace.kind == TraceKind::Evidence && trace.agent_id() == Some(agent.id.as_str())
        });
        let grounds =
            own_evidence.map_or_else(|| self.proposal(topic), |trace| Some(trace.sequence))?;
        Some((topic, grounds))
    }

    /// A message advocating a topic this participant rates below its own
    /// favourite, authored by somebody else.
    fn rival_advocacy(&self, agent: &SimAgent) -> Option<(&TopicId, Sequence)> {
        let mine = agent.score(&agent.favourite);
        self.traces
            .iter()
            .filter(|trace| matches!(trace.kind, TraceKind::Propose | TraceKind::Support))
            .filter(|trace| trace.agent_id().is_some_and(|id| id != agent.id))
            .filter_map(|trace| trace.topic.as_ref().map(|topic| (topic, trace.sequence)))
            .filter(|(topic, _)| **topic != agent.favourite && agent.score(topic) < mine)
            .max_by_key(|(topic, _)| self.backers(topic))
    }
}

impl crate::run::Participant for SimAgent {
    fn id(&self) -> &str {
        &self.id
    }

    fn speak(&mut self, turn: &HiveTurn, visible: &[&SessionMessage]) -> Result<String, String> {
        Ok(self.compose(turn, visible))
    }

    fn cost_unit(&self) -> u32 {
        self.cost_unit
    }
}
