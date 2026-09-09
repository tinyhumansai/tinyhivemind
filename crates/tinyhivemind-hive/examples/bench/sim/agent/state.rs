//! Construction and mutation of a [`SimAgent`]'s state.
//!
//! Everything here builds a member, folds an outside reading into it, or
//! narrows the window it reads through -- the bookkeeping a turn's *decision*
//! in [`super::turn`] reads back out through [`Self::score`] and
//! [`Self::favourite`]. None of it decides what a turn says.

use tinyhivemind_hive::QuorumPolicy;
use tinyhivemind_hive::trace::TopicId;

use crate::context::{ContextBudget, ContextEntry, EntryKind};
use crate::rng::{Rng, mix};

use super::{CheckStyle, SimAgent};
use crate::sim::{DECOY_QUALITY, GROUNDS_WEIGHT, Role, TRUE_QUALITY};

impl SimAgent {
    /// Build a member drawing independent noise around a shared truth.
    ///
    /// `pub(crate)` because [`Room::generate_with`](crate::sim::Room::generate_with)
    /// calls it directly, from outside this module, for `Expertise::Uniform`.
    pub(crate) fn new(
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
            aside_cap: 0,
            asides_spent: 0,
            style: CheckStyle::PLAIN,
            peers: Vec::new(),
            contacted: Vec::new(),
            complied: false,
            ruled_out: Vec::new(),
            context: Vec::new(),
            budget: ContextBudget::UNBOUNDED,
            handled: Vec::new(),
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
        self.context.push(ContextEntry {
            topic: topic.clone(),
            kind: EntryKind::Reading(reading),
        });
        self.recompute_favourite();
        true
    }

    /// Give this member a window to read what it holds through.
    ///
    /// Recomputes the favourite, and must. `Room::pooled` and
    /// `Room::pre_checked` install what a member has absorbed *before* the
    /// episode starts and `recompute_favourite` caches the argmax as they go —
    /// so a budget applied afterwards would narrow `score` while leaving
    /// `favourite` pointing at whatever the member preferred when it could
    /// still read everything. Every turn reads `favourite`, so the window
    /// would have been charged for and then ignored, which is exactly what the
    /// first run of `--context-sweep` showed: `hive+pooled` scoring an
    /// identical 89.4% at a four-row window as at an unbounded one.
    pub(crate) fn set_budget(&mut self, budget: ContextBudget) {
        self.budget = budget;
        self.recompute_favourite();
    }

    /// Install a fact that rules an option out, occupying a row to do it.
    ///
    /// The single place a fact enters a member, so that a fact is subject to
    /// the same window every reading is. That matters more than it sounds: on
    /// a hidden profile the decisive fact is *one* row among many readings that
    /// all agree with each other, and a model of context that charged the
    /// readings but not the fact would make the fact free precisely where it is
    /// load-bearing.
    pub(crate) fn note_fact(&mut self, topic: &TopicId) {
        if !self.ruled_out.contains(topic) {
            self.ruled_out.push(topic.clone());
        }
        self.context.push(ContextEntry {
            topic: topic.clone(),
            kind: EntryKind::Fact,
        });
    }

    /// Rows this member is still carrying, whatever it can still read of them.
    ///
    /// The window's *occupancy*, as against its content: what a long task
    /// costs a participant is exactly this number growing, and it grows for a
    /// soloist holding the whole brief `n` times faster than for one member of
    /// a room of `n`.
    pub(crate) fn held(&self) -> usize {
        self.context.len()
    }

    /// Carry what this member was holding at the end of a previous stage into
    /// this one, ahead of anything it learns here.
    ///
    /// Prepended rather than appended because it *is* older, and the window
    /// model is positional: history belongs at the head, where a compaction
    /// keeping both ends will defend it and a U-curve will read it. Making
    /// history cheap to hold would be assuming away the thing `--stages`
    /// exists to measure.
    ///
    /// The inherited rows name a previous stage's options, which no later
    /// stage shares, so they can never be scored against this stage's
    /// question. They can only take up room, which is the point.
    pub(crate) fn inherit(&mut self, prior: &[ContextEntry]) {
        let mut carried = prior.to_vec();
        carried.append(&mut self.context);
        self.context = carried;
        self.recompute_favourite();
    }

    /// Everything this member is carrying, for the next stage to inherit.
    pub(crate) fn carried(&self) -> &[ContextEntry] {
        &self.context
    }

    /// Rename every option this member holds, for one stage of a chain.
    ///
    /// Its own evaluations, everything it has imported, every fact it holds
    /// and every row in its window, so that nothing is left pointing at a name
    /// the stage no longer uses. The inherited window rows are *not* renamed —
    /// they belong to the stage that wrote them, and that they no longer match
    /// anything is exactly what makes them cost a row and pay nothing.
    pub(crate) fn rename_topics(&mut self, rename: &impl Fn(&TopicId) -> TopicId) {
        for (topic, _) in &mut self.evals {
            *topic = rename(topic);
        }
        for (topic, _, _) in &mut self.imports {
            *topic = rename(topic);
        }
        for topic in &mut self.ruled_out {
            *topic = rename(topic);
        }
        for entry in &mut self.context {
            entry.topic = rename(&entry.topic);
        }
        self.specialty = self.specialty.as_ref().map(rename);
        self.refutes = self.refutes.as_ref().map(rename);
        for (topic, _) in &mut self.expert_elsewhere {
            *topic = rename(topic);
        }
        self.favourite = rename(&self.favourite);
        self.recompute_favourite();
    }

    /// Raise this member's reading of one option.
    ///
    /// Used to poison a stage after the room got the one before it wrong: the
    /// premise is false, so the option consistent with it reads better than it
    /// is. Nothing else about the member changes, and the favourite is
    /// recomputed because a lift can move it.
    pub(crate) fn lift(&mut self, topic: &TopicId, by: i32) {
        for (held, reading) in &mut self.evals {
            if held == topic {
                *reading = reading.saturating_add(by);
            }
        }
        self.recompute_favourite();
    }

    /// Charge this member one row for an exchange it could see but not read.
    pub(crate) fn note_stub(&mut self, topic: &TopicId) {
        self.context.push(ContextEntry {
            topic: topic.clone(),
            kind: EntryKind::Stub,
        });
    }

    /// This member's score for one option, read through its window.
    ///
    /// Only reached when a budget is set. Each surviving row contributes at the
    /// weight its position earns, so a reading buried in the middle of a
    /// crowded window is worth a fraction of the same reading at the edge of a
    /// quiet one, and a row evicted by compaction is worth nothing at all.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a weighted mean of readings is bounded by the readings, which are i32"
    )]
    fn windowed_score(&self, topic: &TopicId, own: i32) -> i32 {
        let kept = self.budget.retained(self.context.len());
        let mut total = f64::from(own);
        let mut divisor = 1.0_f64;
        let mut ruled_out = 0.0_f64;
        // Every row is asked what it is still worth, retained or not: under
        // `Compaction::Evict` an overflowing row answers zero and the loop is
        // the one that ran before folding existed, and under
        // `Compaction::Fold` it answers `fidelity` because a summary keeps no
        // position and so takes no positional discount.
        for (index, entry) in self.context.iter().enumerate() {
            if &entry.topic != topic {
                continue;
            }
            let weight = self.budget.worth(index, &kept);
            if weight <= 0.0 {
                continue;
            }
            match entry.kind {
                EntryKind::Reading(reading) => {
                    total += weight * f64::from(reading);
                    divisor += weight;
                }
                EntryKind::Fact => ruled_out = ruled_out.max(weight),
                // A stub says an exchange happened and nothing about the
                // option, so it moves no score. It has already cost its row.
                EntryKind::Stub => {}
            }
        }
        let pooled = if divisor > 0.0 {
            total / divisor
        } else {
            f64::from(own)
        };
        let discounted = pooled - ruled_out * f64::from(GROUNDS_WEIGHT);
        discounted.round() as i32
    }

    /// Recompute [`Self::favourite`] from every option this member holds a
    /// current [`Self::score`] for.
    ///
    /// `import` calls this on every reading it takes, but a caller that
    /// updates `ruled_out` directly -- `Room::pooled` and `Room::pre_checked`
    /// both do, to install a fact rather than a number -- must call this
    /// itself afterward. `score` reads `ruled_out`, so a ruled-out topic
    /// installed after the last `import` call for a member would otherwise
    /// leave `favourite` pointing at an option that member's own state has
    /// since ruled out, and every turn that reads `favourite` would keep
    /// advocating it.
    ///
    /// `pub(crate)` because `Room::pooled` and `Room::pre_checked`, in the
    /// parent module, install a fact directly and must call this themselves
    /// afterward.
    pub(crate) fn recompute_favourite(&mut self) {
        self.favourite = self
            .evals
            .iter()
            .map(|(topic, _)| topic.clone())
            .max_by_key(|topic| self.score(topic))
            .unwrap_or_else(|| self.favourite.clone());
    }

    /// Tell the participant how many turns it may spend asking one peer for a
    /// second reading before committing to a position. `0` turns the move off.
    ///
    /// `Room::generate_with` leaves every member at `0`, so an arm that opens
    /// no check is bit-identical to one built before the move existed — the
    /// same discipline `set_defer_cap` follows.
    ///
    /// Resets only the exchange state a *floor* check creates — it must not
    /// clear `ruled_out`, because `Room::pre_checked` and `Room::pooled`
    /// write facts into it before the episode opens, and this is called on
    /// that same freshly cloned agent right at `run_episode`'s start. Wiping
    /// it here would silently discard every fact those off-floor controls
    /// preload.
    pub(crate) fn set_aside_cap(&mut self, cap: u32, style: CheckStyle) {
        self.aside_cap = cap;
        self.style = style;
        self.asides_spent = 0;
        self.handled.clear();
        self.contacted.clear();
    }

    /// Tell the participant who else is on its desk.
    ///
    /// A room's membership is not private, and a host hands it to every
    /// participant already; a continuous exchange needs it so a member can
    /// address a peer during the blind round, before anybody has been heard.
    pub(crate) fn set_peers(&mut self, ids: &[&str]) {
        self.peers = ids
            .iter()
            .filter(|id| **id != self.id)
            .map(|id| (*id).to_owned())
            .collect();
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

    /// This participant's own untouched evaluation of one option.
    ///
    /// Distinct from [`Self::score`], which averages in everything absorbed
    /// since. What a member hands a peer has to be the independent signal, or
    /// pooling double-counts.
    pub(crate) fn own_reading(&self, topic: &TopicId) -> i32 {
        self.evals
            .iter()
            .find(|(held, _)| held == topic)
            .map_or(i32::MIN, |(_, own)| *own)
    }

    /// This participant's score for one option: its own reading, averaged
    /// with every outside reading it has taken.
    pub(crate) fn score(&self, topic: &TopicId) -> i32 {
        let Some((_, own)) = self.evals.iter().find(|(held, _)| held == topic) else {
            return i32::MIN;
        };
        if !self.budget.is_unbounded() {
            return self.windowed_score(topic, *own);
        }
        let pooled = match self.imports.iter().find(|(held, _, _)| held == topic) {
            None => *own,
            Some((_, sum, count)) => {
                let total = i64::from(*own).saturating_add(*sum);
                let divisor = i64::from(*count).saturating_add(1);
                i32::try_from(total / divisor).unwrap_or(*own)
            }
        };
        // A fact learned in a private exchange discounts the option for this
        // member exactly as a desk-visible one discounts it for every reader
        // in `View::posterior`, at the same constant. The two channels are
        // therefore worth the same per fact, and the arms differ in how far
        // one fact reaches rather than in how loudly it is stated.
        if self.ruled_out.contains(topic) {
            return pooled.saturating_sub(GROUNDS_WEIGHT);
        }
        pooled
    }
}
