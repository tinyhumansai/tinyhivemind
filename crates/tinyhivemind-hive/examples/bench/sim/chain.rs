//! A room across a chain of stages: renaming, inheriting, and being poisoned.
//!
//! Everything a [`Room`] does when it is one link of a `--stages` chain rather
//! than a whole task. Split from [`super`] at that seam: the parent builds a
//! room and says what its members privately believe, and this says what
//! happens to one when the same members go on to answer another question with
//! the last one still in their windows.
//!
//! The three moves that make a chain a horizon rather than a repeat are here —
//! [`Room::for_stage`] renames so nothing is scored at the wrong stage,
//! [`Room::inheriting`] carries the window forward so history costs something,
//! and [`Room::poisoned`] makes a wrong answer make the next question harder.

use tinyhivemind_hive::trace::TopicId;

use crate::context::{Compaction, ContextBudget};

use super::{Room, SimAgent};

impl Room {
    /// The same room with every option renamed for one stage of a chain.
    ///
    /// Stage `k`'s options are disjoint from every other stage's, so a reading
    /// taken at one stage can never be scored against another's question — it
    /// can only occupy a row. That is what makes a horizon cost something
    /// rather than merely take longer, and it is why the rename happens here
    /// rather than by giving each stage a fresh room: a fresh room would also
    /// forget, and forgetting is the one thing a long task does not let you do.
    pub(crate) fn for_stage(&self, stage: usize) -> Self {
        let rename = |topic: &TopicId| TopicId::from(format!("{}{stage}", topic.0).as_str());
        let mut room = self.clone();
        room.truth = rename(&room.truth);
        room.planted = room.planted.as_ref().map(rename);
        room.experts = room
            .experts
            .iter()
            .map(|(topic, held)| (rename(topic), held.clone()))
            .collect();
        for agent in &mut room.agents {
            agent.rename_topics(&rename);
        }
        room
    }

    /// Carry every member's window contents forward from a previous stage.
    ///
    /// One member at a time, matched by id rather than by position, so a room
    /// whose membership is ordered differently cannot silently hand one
    /// member's history to another.
    pub(crate) fn inheriting(&self, prior: &Self) -> Self {
        let mut room = self.clone();
        for agent in &mut room.agents {
            if let Some(held) = prior.agents.iter().find(|peer| peer.id == agent.id) {
                agent.inherit(held.carried());
            }
        }
        room
    }

    /// Mean rows a member of this room is carrying.
    #[expect(
        clippy::cast_precision_loss,
        reason = "a room holding more rows than an f64 can count is not a room"
    )]
    pub(crate) fn held(&self) -> f64 {
        if self.agents.is_empty() {
            return 0.0;
        }
        let total: usize = self.agents.iter().map(SimAgent::held).sum();
        total as f64 / self.agents.len() as f64
    }

    /// Lift every member's reading of one option, as a room reasoning from a
    /// premise it got wrong would.
    ///
    /// The mechanism that makes a chain *compound* rather than merely repeat.
    /// A stage decided wrongly does not leave the next stage untouched: it
    /// leaves every member believing something false, and the option
    /// consistent with that falsehood looks better than it is. Recovery stays
    /// possible — the lift is bounded well below what would make the truth
    /// unreachable — and is deliberately hard.
    pub(crate) fn poisoned(&self, lift: i32) -> Self {
        let mut room = self.clone();
        let decoy = room
            .agents
            .first()
            .and_then(|agent| {
                agent
                    .evals
                    .iter()
                    .map(|(topic, _)| topic)
                    .find(|topic| **topic != room.truth)
            })
            .cloned();
        let Some(decoy) = decoy else { return room };
        for agent in &mut room.agents {
            agent.lift(&decoy, lift);
        }
        room
    }

    /// Charge every member one row per option for the brief it was handed.
    ///
    /// Without this a member's own reading is free: it lives in `evals`, which
    /// no window touches, so a room of five carries *nothing* while a soloist
    /// handed the same five briefs carries all of it. That asymmetry is an
    /// artifact of where the harness happens to store a number, not a property
    /// of either arm, and on a long task it is the whole quantity under test.
    ///
    /// The rows are [`EntryKind::Stub`]s: they occupy a position and move no
    /// score, because the reading they stand for is already counted through
    /// `evals`. What they buy is that it is counted *as something held*.
    ///
    /// Only `--stages` calls this, so every other arm carries exactly what it
    /// carried before.
    ///
    /// [`EntryKind::Stub`]: crate::context::EntryKind::Stub
    pub(crate) fn charge_brief(&mut self) {
        let topics: Vec<TopicId> = self
            .agents
            .first()
            .map(|agent| agent.evals.iter().map(|(topic, _)| topic.clone()).collect())
            .unwrap_or_default();
        for agent in &mut self.agents {
            for topic in &topics {
                agent.note_stub(topic);
            }
        }
    }

    /// Give every member of this room the same window.
    pub(crate) fn set_budget(&mut self, budget: ContextBudget) {
        self.budget = budget;
        for agent in &mut self.agents {
            agent.set_budget(budget);
        }
    }

    /// Change what happens to a row this room's window has no space for,
    /// leaving its capacity and its curve alone.
    pub(crate) fn set_compaction(&mut self, compaction: Compaction) {
        let budget = ContextBudget {
            compaction,
            ..self.budget
        };
        self.set_budget(budget);
    }
}
