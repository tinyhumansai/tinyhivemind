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
    HiveTurn, Phase, Sequence, SessionMessage,
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
/// Backers a topic needs before a participant treats it as contending.
const CONTENDER_BACKERS: usize = 2;

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
        let truth = names.get(truth_index).cloned().unwrap_or(TopicId::from("stage"));

        let members = MEMBER_ROLES
            .iter()
            .take(agents)
            .enumerate()
            .map(|(index, (id, role))| {
                SimAgent::new(id, *role, seed, index, &names, &truth, noise)
            })
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
            rng: Rng::seeded(mix(seed, 0xA11C_E ^ index as u64)),
        }
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
        let view = View::fold(visible);

        // A commit turn records what the room actually carried, not what this
        // member would have preferred. Refusing to record it is how a room
        // spends its whole budget without terminating.
        if turn.phase == Phase::Commit {
            if let Some((topic, grounds)) = view.leading(self) {
                return format!("!commit #{topic} ^{grounds} Recording the decision the room reached.");
            }
        }

        // Real participants do not speak the grammar on every turn. Modelling
        // that is the difference between benchmarking the protocol and
        // benchmarking a formatter.
        if self.rng.chance(NONCOMPLIANCE) {
            return format!("Thinking about this; {} still looks strongest to me.", self.favourite);
        }

        // Two options are carrying at once and the room cannot settle while
        // both do. Cross-inhibition is the mechanism the library provides for
        // exactly this: object to a *message*, which silences its author as an
        // advocate rather than debiting the option. Subtracting from a score
        // could not break a tie between two equally supported options.
        if let Some((topic, target, grounds)) = view.weaker_contender(self) {
            return format!(
                "!object >{target} ^{grounds} I rate {topic} below the other option carrying here."
            );
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
            |grounds| format!("!evidence ^{grounds} Prior rollouts of this shape behaved the same way."),
        )
    }
}

/// The traces one turn can actually see, folded once.
struct View {
    traces: Vec<Trace>,
}

impl View {
    fn fold(visible: &[&SessionMessage]) -> Self {
        let mut traces: Vec<Trace> = visible
            .iter()
            .flat_map(|message| {
                resolve(&message.content, None, &message.author, message.sequence)
            })
            .collect();
        traces.sort_by_key(|trace| (trace.sequence, trace.offset));
        Self { traces }
    }

    /// The sequence that first proposed a topic, if it is on the floor.
    fn proposal(&self, topic: &TopicId) -> Option<Sequence> {
        self.traces
            .iter()
            .find(|trace| trace.kind == TraceKind::Propose && trace.topic.as_ref() == Some(topic))
            .map(|trace| trace.sequence)
    }

    /// The proposed option this participant rates highest once the room's own
    /// independent backing is weighed against its private evaluation.
    fn best_proposal(&self, agent: &SimAgent) -> Option<(&TopicId, Sequence)> {
        self.traces
            .iter()
            .filter(|trace| trace.kind == TraceKind::Propose)
            .filter_map(|trace| trace.topic.as_ref().map(|topic| (topic, trace.sequence)))
            .max_by_key(|(topic, _)| self.posterior(agent, topic))
    }

    /// A participant's private score for an option, updated by how many peers
    /// independently backed it.
    fn posterior(&self, agent: &SimAgent, topic: &TopicId) -> i32 {
        let peers = self
            .backing(topic)
            .iter()
            .filter(|backer| **backer != agent.id)
            .count();
        let peers = i32::try_from(peers).unwrap_or(0);
        agent.score(topic).saturating_add(peers.saturating_mul(SOCIAL_WEIGHT))
    }

    /// The two options carrying the most backing, when both are contenders and
    /// this participant rates one clearly below the other: the message to
    /// object to, and grounds to cite.
    fn weaker_contender(&self, agent: &SimAgent) -> Option<(&TopicId, Sequence, Sequence)> {
        let mut contenders: Vec<(&TopicId, usize)> = Vec::new();
        for trace in &self.traces {
            if trace.kind != TraceKind::Propose {
                continue;
            }
            let Some(topic) = trace.topic.as_ref() else {
                continue;
            };
            if contenders.iter().any(|(held, _)| *held == topic) {
                continue;
            }
            let backing = self.backers(topic);
            if backing >= CONTENDER_BACKERS {
                contenders.push((topic, backing));
            }
        }
        if contenders.len() < 2 {
            return None;
        }
        let best = contenders
            .iter()
            .max_by_key(|(topic, backing)| (*backing, self.posterior(agent, topic)))?;
        let worst = contenders
            .iter()
            .filter(|(topic, _)| *topic != best.0)
            .min_by_key(|(topic, backing)| (*backing, self.posterior(agent, topic)))?;
        if self.posterior(agent, worst.0) >= self.posterior(agent, best.0) {
            return None;
        }
        // Silence one advocate of the weaker option: someone other than this
        // participant, who actually advocated it in the message being named.
        let target = self.traces.iter().find(|trace| {
            matches!(trace.kind, TraceKind::Propose | TraceKind::Support)
                && trace.topic.as_ref() == Some(worst.0)
                && trace.agent_id().is_some_and(|id| id != agent.id)
        })?;
        let grounds = self.proposal(best.0)?;
        Some((worst.0, target.sequence, grounds))
    }

    /// Every distinct agent that advocated a topic.
    fn backing(&self, topic: &TopicId) -> Vec<&str> {
        let mut seen: Vec<&str> = Vec::new();
        for trace in &self.traces {
            if !matches!(trace.kind, TraceKind::Propose | TraceKind::Support)
                || trace.topic.as_ref() != Some(topic)
            {
                continue;
            }
            if let Some(agent) = trace.agent_id()
                && !seen.contains(&agent)
            {
                seen.push(agent);
            }
        }
        seen
    }

    /// Whether an agent already advocated a topic.
    fn has_backed(&self, agent_id: &str, topic: &TopicId) -> bool {
        self.traces.iter().any(|trace| {
            matches!(trace.kind, TraceKind::Propose | TraceKind::Support)
                && trace.topic.as_ref() == Some(topic)
                && trace.agent_id() == Some(agent_id)
        })
    }

    /// The topic carrying the most distinct advocates, breaking ties by the
    /// participant's own evaluation, with a sequence to cite as grounds.
    fn leading(&self, agent: &SimAgent) -> Option<(&TopicId, Sequence)> {
        let mut best: Option<(&TopicId, Sequence, usize, i32)> = None;
        for trace in &self.traces {
            if trace.kind != TraceKind::Propose {
                continue;
            }
            let Some(topic) = trace.topic.as_ref() else {
                continue;
            };
            let backing = self.backers(topic);
            let score = self.posterior(agent, topic);
            let better = best.is_none_or(|(_, _, held_backing, held_score)| {
                (backing, score) > (held_backing, held_score)
            });
            if better {
                best = Some((topic, trace.sequence, backing, score));
            }
        }
        best.map(|(topic, sequence, _, _)| (topic, sequence))
    }

    /// How many distinct agents advocated a topic.
    fn backers(&self, topic: &TopicId) -> usize {
        self.backing(topic).len()
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
