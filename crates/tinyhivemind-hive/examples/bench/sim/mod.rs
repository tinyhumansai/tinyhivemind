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
//!
//! # Module layout
//!
//! The room and its tuning constants live here; member generation
//! ([`generation`]), the participant itself ([`agent`]), and the read-only
//! projection a turn composes against ([`view`]) each get their own module so
//! that no single file outgrows what a reader can hold in mind at once.

use tinyhivemind_hive::trace::TopicId;

use crate::context::ContextBudget;
use crate::rng::{Rng, mix};

mod agent;
mod chain;
mod facet;
mod generation;
mod view;

use agent::Holdings;
use generation::{
    MemberDraw, draw_expertise, hidden_profile_agent, selfcheck_uniform, specialist_agent,
};

pub(crate) use agent::{CheckStyle, SimAgent};
pub(crate) use view::check_selfcheck;

/// How far a stage decided wrongly lifts the next stage's decoy.
///
/// Bounded on both sides, like `HIDDEN_LIFT` and `GROUNDS_WEIGHT` before it.
/// *Below* the 60-point gap between the true option and a decoy, so a poisoned
/// room can still recover — a chain in which one wrong answer made every later
/// one unwinnable would measure nothing after the first mistake. *Above* zero
/// by enough to matter against the noise, or a chain would merely be a repeat
/// with extra steps. Forty is three quarters of the way to unrecoverable.
pub(crate) const POISON_LIFT: i32 = 40;

/// Names drawn on, in order, for a room's options.
pub(crate) const TOPIC_NAMES: [&str; 8] = [
    "stage", "ship", "revert", "shadow", "canary", "freeze", "split", "pilot",
];

/// The largest room this harness will build, as a `u32`.
///
/// Declared first and widened into [`MAX_MEMBERS`] rather than the other way
/// round, so the refutation cap below is a plain constant rather than a cast
/// that has to argue it cannot truncate.
const MAX_MEMBERS_U32: u32 = 256;

/// The largest room this harness will build.
///
/// Not a property of the library, which has no room-size limit — a bound on
/// what a *benchmark* will spend. Every arm decides the same rooms, so one
/// swept size costs every arm at once, and a room of a thousand members would
/// spend minutes per size to answer a question the shape of the curve already
/// answers by a hundred.
pub(crate) const MAX_MEMBERS: usize = MAX_MEMBERS_U32 as usize;

/// Names and roles drawn on, in order, for a room's first eight members.
///
/// Beyond them [`member_at`] generates, because a fixed table is a cap on room
/// size dressed as a convenience: `--agents` clamped to 8 for no reason other
/// than that this array ends there.
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

/// The name and role of member `index`, for a room of any size.
///
/// The first eight keep the names the recorded benchmarks were written
/// against, so every number at those sizes is reproduced exactly rather than
/// approximately. Past them the roles cycle in the same order — a room of
/// thirty-two is four of the same rotation, which keeps the mix of proposers,
/// critics and archivists flat as the room grows instead of letting one role
/// dominate a large desk by accident.
pub(crate) fn member_at(index: usize) -> (String, Role) {
    MEMBER_ROLES.get(index).map_or_else(
        || {
            let (_, role) = MEMBER_ROLES[index % MEMBER_ROLES.len()];
            (format!("seat{index}"), role)
        },
        |(name, role)| ((*name).to_string(), *role),
    )
}

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
///
/// It is [`MAX_MEMBERS`] rather than the room's own size on purpose: what it
/// decides is whether a cap is *conceivably* satisfiable, and making it depend
/// on the room would change how a member behaves at one size for a reason that
/// has nothing to do with the room being larger. The recorded numbers at five
/// members are unaffected either way, because every cap the arms actually pass
/// is far below both.
const REACHABLE_REFUTATION_CAP: u32 = MAX_MEMBERS_U32;
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

/// How close this member's top two options must be before it is worth a turn
/// asking one peer what they read.
///
/// A member that already separates its two best options by more than this has
/// nothing a second noisy reading would settle, and spending a turn on the
/// question would be spending it to learn nothing. Set at half the gap that
/// separates the genuinely best option from a decoy: inside it the member
/// genuinely cannot tell, outside it it can.
const ASIDE_UNCERTAINTY: i32 = (TRUE_QUALITY - DECOY_QUALITY) / 2;

/// What a member writes into the reading it hands a peer.
///
/// Parsed back out by the peer, so the two halves of the exchange share one
/// spelling. The number is this member's own score for the topic, which is
/// exactly what a second opinion is.
const ASIDE_READS: &str = "reads";

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
    /// One member owns the whole question: it reads every option far more
    /// tightly than anybody else, and everybody else's read of every option
    /// widens to match.
    ///
    /// [`Specialists`] scatters expertise across the room at random, one
    /// topic each. This concentrates it, by construction rather than by
    /// draw — which is what a *role* is. It exists for `--facets`, where a
    /// task is several sub-questions at once and each is somebody's job: the
    /// owner is `facet % members`, known to the harness before any member
    /// reads anything, so no arm learns who it is from scoring data.
    ///
    /// [`Specialists`]: Expertise::Specialists
    Roles {
        /// Index of the member whose question this is.
        owner: usize,
    },
}

/// One simulated room: the options, which is best, and who is in it.
#[derive(Clone, Debug)]
pub(crate) struct Room {
    /// The option that is genuinely best.
    pub(crate) truth: TopicId,
    /// The participants.
    pub(crate) agents: Vec<SimAgent>,
    /// The window every member in this room reads through.
    ///
    /// Carried on the room rather than threaded through `run_episode_checking`
    /// so that `pooled` and `pre_checked`, which clone the room to install what
    /// a member has absorbed, cannot install information into a room and then
    /// forget what it costs to hold.
    pub(crate) budget: ContextBudget,
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
        let agent_count = agents.clamp(2, MAX_MEMBERS);
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
        let members: Vec<SimAgent> = (0..agent_count)
            .map(|index| {
                let (id, role) = member_at(index);
                match expertise {
                    Expertise::Uniform => {
                        SimAgent::new(&id, role, seed, index, &names, &truth, noise)
                    }
                    Expertise::Specialists { .. } => {
                        specialist_agent(&id, role, index, &draw, &expert_of, cost_tiers)
                    }
                    Expertise::HiddenProfile => {
                        hidden_profile_agent(&id, role, index, &draw, decisive_index, planted_index)
                    }
                    Expertise::Roles { .. } => {
                        specialist_agent(&id, role, index, &draw, &expert_of, cost_tiers)
                    }
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
            budget: ContextBudget::UNBOUNDED,
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

    /// The same room, with every private reading and every refuting fact
    /// already in every member's hands, at no turn cost at all.
    ///
    /// This is the **ceiling** on pairwise exchange, and it is the control
    /// the aside arms were missing. Those arms confound two things: what a
    /// check is worth, and what the turns it spends cost. `swarm.rs` already
    /// solves the same problem one level up with `pooled`, which hands every
    /// desk every other desk's readings for free and bounds what crossing a
    /// channel could ever buy. This is that arm for a pair inside one desk.
    ///
    /// It is deliberately more generous than any protocol could be: every
    /// member absorbs every peer's own reading of every option, and every
    /// refutation any peer holds, before the episode opens. No sequence of
    /// asides can beat it, so if the room is no better here the mechanism is
    /// not what is limiting the room.
    /// The same room, read through a window of `budget`.
    pub(crate) fn with_budget(&self, budget: ContextBudget) -> Self {
        let mut room = self.clone();
        room.budget = budget;
        room
    }

    /// The same room, with every member having absorbed every peer's own
    /// reading of every option, and any fact a peer holds, before the episode
    /// opens.
    ///
    /// See [`Self::with_budget`]'s doc comment above, which this shares a
    /// paragraph break with: it is the ceiling this pairwise mechanism can
    /// ever reach, at no turn cost at all.
    pub(crate) fn pooled(&self) -> Self {
        let mut room = self.clone();
        let readings: Vec<Holdings> = self
            .agents
            .iter()
            .map(|agent| (agent.evals.clone(), agent.refutes.clone()))
            .collect();
        for (index, agent) in room.agents.iter_mut().enumerate() {
            for (peer, (evals, refutes)) in readings.iter().enumerate() {
                if peer == index {
                    continue;
                }
                for (topic, reading) in evals {
                    agent.import(topic, *reading);
                }
                if let Some(topic) = refutes {
                    agent.note_fact(topic);
                }
            }
            // A refutation installed after this agent's last `import` call
            // above -- always true for whichever peer is processed last --
            // would otherwise leave `favourite` pointing at an option this
            // agent's own `ruled_out` has since discounted.
            agent.recompute_favourite();
        }
        room
    }

    /// The same room, with each member having spent `cap` pairwise checks
    /// **before the episode opens**, off the floor.
    ///
    /// [`Self::pooled`] bounds what any exchange could be worth; this bounds
    /// what *the aside arms' own exchange* is worth once it stops competing
    /// with the deliberation for turns. Same trigger as
    /// [`SimAgent::open_check`] — a member that cannot separate its two best
    /// options — same bounded number of contacts, same payload. The one
    /// difference is that the contact does not consume a floor turn.
    ///
    /// The peer is chosen at least as well as the on-floor arm chooses one:
    /// whoever actually holds the fact, and otherwise the next member round.
    /// So an on-floor arm cannot be losing to worse targeting than this.
    pub(crate) fn pre_checked(&self, cap: u32, evidence: bool) -> Self {
        let mut room = self.clone();
        let readings: Vec<Holdings> = self
            .agents
            .iter()
            .map(|agent| (agent.evals.clone(), agent.refutes.clone()))
            .collect();
        let count = room.agents.len();
        for (index, agent) in room.agents.iter_mut().enumerate() {
            for spent in 0..cap {
                let Some(topic) = agent.uncertain_about() else {
                    break;
                };
                // Whoever holds the fact, and otherwise the next member round.
                let peer = readings
                    .iter()
                    .position(|(_, refutes)| refutes.as_ref() == Some(&topic))
                    .filter(|peer| *peer != index)
                    .unwrap_or_else(|| (index + 1 + spent as usize) % count.max(1));
                if peer == index {
                    break;
                }
                let Some((evals, refutes)) = readings.get(peer) else {
                    break;
                };
                // A fact-bearing peer hands over the fact *instead of* its
                // reading, matching the on-floor `CheckStyle::FACT` exchange
                // in `absorb` -- otherwise `hive+fact°` would carry both the
                // fact and the reading `hive+fact` never gets to average in.
                if evidence && refutes.as_ref() == Some(&topic) {
                    agent.note_fact(&topic);
                } else if let Some((_, reading)) = evals.iter().find(|(held, _)| *held == topic) {
                    agent.import(&topic, *reading);
                }
            }
            // `import` already recomputes `favourite` on every reading taken,
            // which covers today's data (every peer holds a reading for
            // every topic). Recomputing again here is a cheap guarantee
            // against the one case that would not: a `ruled_out` fact
            // installed above with no matching `import` call to refresh it.
            agent.recompute_favourite();
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
