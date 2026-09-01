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

use std::fmt::Write as _;

use tinyhivemind_hive::{
    HiveTurn, Phase, QuorumPolicy, Sequence, SessionMessage,
    quorum::{TopicStanding, standings},
    trace::{TopicId, Trace, TraceKind, resolve},
};

use crate::rng::{Rng, mix};

/// Names drawn on, in order, for a room's options.
const TOPIC_NAMES: [&str; 8] = [
    "stage", "ship", "revert", "shadow", "canary", "freeze", "split", "pilot",
];

/// Names and roles drawn on, in order, for a room's members.
const MEMBER_ROLES: [(&str, Role); 8] = [
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

/// One simulated room: the options, which is best, and who is in it.
#[derive(Clone, Debug)]
pub(crate) struct Room {
    /// The option that is genuinely best.
    pub(crate) truth: TopicId,
    /// The participants.
    pub(crate) agents: Vec<SimAgent>,
}

impl Room {
    /// Generate a reproducible room.
    ///
    /// `noise` is the half-width of the uniform error on each private
    /// evaluation: at `0` every member already knows the answer, and as it
    /// approaches the 60-point quality gap the members become individually
    /// unreliable while remaining collectively informative.
    pub(crate) fn generate(seed: u64, agents: usize, topics: usize, noise: u32) -> Self {
        let topics = topics.clamp(2, TOPIC_NAMES.len());
        let agents = agents.clamp(2, MEMBER_ROLES.len());
        let names: Vec<TopicId> = TOPIC_NAMES
            .iter()
            .take(topics)
            .map(|name| TopicId::from(*name))
            .collect();
        // The truth is placed by the seed rather than at a fixed index, so no
        // arm of the benchmark can score by preferring the first option.
        let truth_index = usize::try_from(mix(seed, 0x7275_7468) % (topics as u64)).unwrap_or(0);
        let truth = names
            .get(truth_index)
            .cloned()
            .unwrap_or(TopicId::from("stage"));

        let members = MEMBER_ROLES
            .iter()
            .take(agents)
            .enumerate()
            .map(|(index, (id, role))| SimAgent::new(id, *role, seed, index, &names, &truth, noise))
            .collect();

        Self {
            truth,
            agents: members,
        }
    }

    /// The member ids, in desk order.
    pub(crate) fn member_ids(&self) -> Vec<&str> {
        self.agents.iter().map(|agent| agent.id.as_str()).collect()
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
    /// Its own argmax over `evals`.
    favourite: TopicId,
    /// Drives noncompliance only; never the private evaluations.
    rng: Rng,
    /// The room's quorum rule, which is public: a participant is entitled to
    /// know how many grounded supporters settle a question, and reads the
    /// medium through the library's own fold rather than a private imitation
    /// of it.
    quorum: QuorumPolicy,
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
        let favourite = evals
            .iter()
            .max_by_key(|(_, score)| *score)
            .map_or_else(|| TopicId::from("stage"), |(topic, _)| topic.clone());
        Self {
            id: id.to_owned(),
            role,
            evals,
            favourite,
            rng: Rng::seeded(mix(seed, 0x000A_11CE ^ index as u64)),
            quorum: QuorumPolicy::DEFAULT,
        }
    }

    /// Tell the participant which quorum rule the room is running.
    pub(crate) fn set_quorum(&mut self, quorum: QuorumPolicy) {
        self.quorum = quorum;
    }

    /// The option this participant would pick with no deliberation at all.
    ///
    /// This is what the single-responder arms of the benchmark get: one
    /// member's unaided judgement.
    pub(crate) fn favourite(&self) -> &TopicId {
        &self.favourite
    }

    /// This participant's private score for one option.
    fn score(&self, topic: &TopicId) -> i32 {
        self.evals
            .iter()
            .find(|(held, _)| held == topic)
            .map_or(i32::MIN, |(_, score)| *score)
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

        // Back the best option currently on the floor, weighing this member's
        // own signal against how many peers independently backed it. This is
        // the step that pools information across the room.
        if let Some((topic, grounds)) = view.best_proposal(self)
            && !view.has_backed(&self.id, topic)
        {
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
    /// members independently back it.
    fn posterior(&self, agent: &SimAgent, topic: &TopicId) -> i32 {
        let peers = self
            .backing(topic)
            .iter()
            .filter(|backer| **backer != agent.id)
            .count();
        let peers = i32::try_from(peers).unwrap_or(0);
        agent
            .score(topic)
            .saturating_add(peers.saturating_mul(SOCIAL_WEIGHT))
    }

    /// Whether the library still counts this agent as backing a topic.
    fn has_backed(&self, agent_id: &str, topic: &TopicId) -> bool {
        self.backing(topic).iter().any(|held| held == agent_id)
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
}
