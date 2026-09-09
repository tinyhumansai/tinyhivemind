//! A room as one facet of a task that has several at once.
//!
//! [`super::chain`] is this module's sibling in depth: it says what happens to
//! a room when the same members answer another question *after* this one.
//! This says what happens when a task is several questions *at the same time*
//! — a design decision and a rollout decision and a naming decision, each
//! needing a different kind of attention, none of them waiting on the others.
//!
//! That difference is the whole reason the module exists. A chain compounds
//! through time, so a wrong answer poisons the next question. A spread of
//! facets does not: the facets are independent, and what compounds is only
//! the conjunction — a task is done when *every* facet is right. So there is
//! no [`Room::poisoned`] here and there does not need to be. What there is
//! instead is [`Room::for_facet`], which keeps one facet's options from ever
//! being scored against another's, exactly as [`Room::for_stage`] does for a
//! stage, and for the same reason: a reading taken on the rollout question can
//! only ever occupy a row in the naming question's window.
//!
//! [`Room::poisoned`]: super::Room::poisoned
//! [`Room::for_stage`]: super::Room::for_stage

use tinyhivemind_hive::trace::TopicId;

use super::Room;

impl Room {
    /// The same room with every option renamed for one facet of a task.
    ///
    /// Suffixed distinctly from [`Room::for_stage`]'s bare index, so a run
    /// that ever combined the two could not have a facet's option collide
    /// with a stage's. Nothing does today; the cost of making it impossible
    /// is one character.
    ///
    /// [`Room::for_stage`]: super::Room::for_stage
    pub(crate) fn for_facet(&self, facet: usize) -> Self {
        let rename = |topic: &TopicId| TopicId::from(format!("{}f{facet}", topic.0).as_str());
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

    /// Rows the member at `index` is carrying, or zero for a room that has no
    /// such member.
    ///
    /// A spread of facets is the one regime where the room's *mean* holding
    /// is the wrong number: under `hive+fold` only the facet's owner reads
    /// anything, so averaging its window with four idle ones would report a
    /// fifth of the load the arm actually carries. [`Room::held`] stays the
    /// right number for every arm where all members read.
    ///
    /// [`Room::held`]: super::Room::held
    pub(crate) fn held_by(&self, index: usize) -> usize {
        self.agents.get(index).map_or(0, super::SimAgent::held)
    }
}
