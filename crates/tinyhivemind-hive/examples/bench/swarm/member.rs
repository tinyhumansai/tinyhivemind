//! The simulated participant that can spend a turn asking another channel.
//!
//! [`SwarmSim`] wraps [`crate::sim::SimAgent`] — the same arithmetic
//! participant the single-desk benchmark uses, unchanged — with the one thing
//! a federation adds: it knows which channel it is on, and it will spend a
//! turn asking another one before backing anything in its own. This module
//! also holds the small bookkeeping [`super::drive_swarm`] needs once a desk
//! finishes: recording how it ended, and reading off what the federation as a
//! whole settled on.

use std::fmt::Write as _;

use tinyhivemind_hive::{
    HiveTurn, SessionMessage,
    referral::{Referral, ReferralKind},
    trace::TopicId,
};

use super::format::{Reading, readings, restate};
use super::{Channel, DeskOutcome, SwarmMember};
use crate::federation::Federation;
use crate::run::Ending;

/// A simulated member of a federation.
///
/// It is a [`crate::sim::SimAgent`] — the same arithmetic participant the
/// single-desk benchmark uses, unchanged — plus the one thing a federation
/// adds: it knows which channel it is on, and it will spend a turn asking
/// another one.
pub(super) struct SwarmSim {
    /// The arithmetic participant this member wraps.
    agent: crate::sim::SimAgent,
    /// This member's own desk, by display name.
    here: String,
    /// Every desk in the federation, id and display name.
    directory: Vec<(String, String)>,
    /// The options, so a reading can cover the slate rather than one option.
    topics: Vec<TopicId>,
}

impl SwarmSim {
    /// Seat one simulated agent in its federation.
    pub(super) fn new(federation: &Federation, agent: crate::sim::SimAgent) -> Self {
        let here = federation
            .desk_of(&agent.id)
            .map_or_else(String::new, |desk| desk.name.clone());
        Self {
            agent,
            here,
            directory: federation
                .desks
                .iter()
                .map(|desk| (desk.id.clone(), desk.name.clone()))
                .collect(),
            topics: federation.topics.clone(),
        }
    }

    /// How this member reads every option, written the way a member would.
    fn slate(&self) -> String {
        let mut line = format!("{} reads", self.here);
        for (at, topic) in self.topics.iter().enumerate() {
            let separator = if at == 0 { "" } else { "," };
            let _ = write!(line, "{separator} #{topic} at {}", self.agent.score(topic));
        }
        line.push('.');
        line
    }

    /// A desk's display name.
    fn desk_name<'a>(&'a self, desk_id: &'a str) -> &'a str {
        self.directory
            .iter()
            .find(|(id, _)| id == desk_id)
            .map_or(desk_id, |(_, name)| name.as_str())
    }
}

impl SwarmMember for SwarmSim {
    fn id(&self) -> &str {
        &self.agent.id
    }

    /// Ask another channel before backing anything here.
    ///
    /// The rule is one a member could defend out loud: *I will not put an
    /// option on the floor on the strength of what my own desk thinks, when
    /// nobody outside this desk has told me anything about it.* The question
    /// goes out **before** the desk has backed anything, and that turns out to
    /// be the whole ballgame. A desk whose members share a bias reaches quorum
    /// inside its own blind opening round, and an answer arriving after that is
    /// information the desk has already voted past. An earlier version of this
    /// harness asked after proposing, and every desk committed to its own decoy
    /// with the correction sitting three lines below the decision.
    fn ask(&mut self, peers: &[&str]) -> Option<String> {
        let peer = peers.first()?;
        Some(format!(
            "@#{peer} We are about to back #{} here. {} How do you read them?",
            self.agent.favourite(),
            self.slate(),
        ))
    }

    fn speak(&mut self, turn: &HiveTurn, visible: &[SessionMessage]) -> Result<String, String> {
        crate::run::Participant::speak(&mut self.agent, turn, visible)
    }

    /// Answer a question that arrived from another channel.
    ///
    /// The answer covers the whole slate rather than the one option asked
    /// about. That is what a real cross-team answer looks like, and it is also
    /// what makes the pooling work: an outside reading of one option halves
    /// that option's bias and leaves every other option untouched, which moves
    /// the argmax around without improving it.
    fn answer(
        &mut self,
        incoming: &Referral,
        _visible: &[SessionMessage],
    ) -> Result<String, String> {
        let carried = readings(&incoming.content);
        if carried.is_empty() {
            return Ok("!question I cannot read a rating out of that.".to_owned());
        }
        Ok(match incoming.kind {
            // Repeat what was asked and add this desk's own reading, so the
            // whole exchange is legible to everybody here rather than only to
            // the two agents in it.
            ReferralKind::Forward => {
                format!("!evidence {} {}", restate(&carried), self.slate())
            }
            // Carrying an answer home: only the far desk's readings are news.
            ReferralKind::Return => {
                let from = self.desk_name(&incoming.from.desk_id).to_owned();
                let theirs: Vec<Reading> = carried
                    .into_iter()
                    .filter(|reading| reading.desk == from)
                    .collect();
                if theirs.is_empty() {
                    return Ok("!question That answer carried no rating I can use.".to_owned());
                }
                format!("!evidence {}", restate(&theirs))
            }
        })
    }

    /// Publish this desk's reading of the whole slate.
    ///
    /// The same line an answer carries, with nobody having asked for it. It is
    /// `!evidence` because that is what it is — a reading, not a position —
    /// and because the receiving desk folds it as neither: the author is not a
    /// member there, so the trace is filtered out of that episode's standings
    /// and reaches only [`SwarmMember::absorb`].
    fn publish(&mut self) -> Option<String> {
        Some(format!("!evidence {}", self.slate()))
    }

    /// Fold every outside reading a line carries into this member's own view.
    ///
    /// This is the step a channel boundary otherwise prevents, and it is
    /// deliberately the *only* thing crossing one has ever done here: a reading
    /// changes what a member believes, and the member still has to spend a turn
    /// saying so before the room counts it.
    fn absorb(&mut self, content: &str) {
        for reading in readings(content) {
            if reading.desk == self.here {
                continue;
            }
            if !self.directory.iter().any(|(_, name)| *name == reading.desk) {
                continue;
            }
            self.agent.import(&reading.topic, reading.value);
        }
    }
}

/// Record how one desk ended.
pub(super) fn outcome(
    channels: &[Channel],
    desk: usize,
    ending: Ending,
    decided: Option<TopicId>,
) -> DeskOutcome {
    DeskOutcome {
        name: channels[desk].name.clone(),
        ending,
        decided,
    }
}

/// The option most desks settled on, or `None` when they are tied.
///
/// A tie is not a decision. Breaking it by desk order would hand the federation
/// a win it did not earn, which is the kind of quiet thumb on the scale a
/// benchmark exists to avoid.
pub(super) fn plurality(desks: &[DeskOutcome]) -> Option<TopicId> {
    let mut tally: Vec<(&TopicId, u32)> = Vec::new();
    for decided in desks.iter().filter_map(|desk| desk.decided.as_ref()) {
        match tally.iter_mut().find(|(topic, _)| *topic == decided) {
            Some(entry) => entry.1 = entry.1.saturating_add(1),
            None => tally.push((decided, 1)),
        }
    }
    let most = tally.iter().map(|(_, count)| *count).max()?;
    let mut leaders = tally.iter().filter(|(_, count)| *count == most);
    let leader = leaders.next()?;
    if leaders.next().is_some() {
        return None;
    }
    Some(leader.0.clone())
}
