//! Aggregation and the tables the benchmark prints.
//!
//! Every rate is reported over the whole sample, including the episodes that
//! never decided anything: an arm cannot buy accuracy by declining to answer.
//!
//! The formatting helpers that turn an [`Aggregate`] into the printed and
//! `--json` tables live in [`format`]; the small numeric statistics
//! ([`stats::lossy`], the bootstrap percentile, and the rank-tie arithmetic
//! behind [`spearman_milli`]) live in [`stats`]. Both submodules are private:
//! everything a caller outside this module needs is re-exported here.

mod format;
mod stats;

use std::fmt::Write as _;
use std::time::Duration;

use format::{
    count, dash_unless, dash_unless_2, deliberates, duration, json_f64, json_f64_if, row,
};
use stats::lossy;

// Re-exported so `compare.rs`, `main.rs` and the sweeps keep importing the
// benchmark's statistics from `metrics`, where they are used, rather than
// having to know which file inside it they now live in.
pub(crate) use stats::{paired_bootstrap, spearman_milli, wilson};

use crate::arms::ArmReport;
use crate::run::{Ending, EpisodeReport};

/// Running totals over a sample of episodes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Aggregate {
    /// The model every episode folded in here was priced against.
    ///
    /// Held per-aggregate rather than passed to [`Aggregate::add`] because an
    /// arm's totals and the constants they were priced under are one fact: a
    /// row of the table reporting tokens computed at one point and a wall
    /// clock computed at another would be a number nothing produced. Every
    /// aggregate in one run carries the same model, which [`Aggregate::merge`]
    /// asserts in debug builds.
    pub(crate) model: crate::cost::CostModel,
    /// What the sample cost in tokens and wall clock, priced by [`Self::model`].
    pub(crate) cost: crate::cost::Cost,
    /// Episodes in the sample.
    pub(crate) episodes: u32,
    /// Episodes that ended in a recorded decision.
    pub(crate) converged: u32,
    /// Episodes that tied with nobody left to break it.
    pub(crate) deadlocked: u32,
    /// Episodes that spent their budget.
    pub(crate) exhausted: u32,
    /// Episodes where nobody cleared their threshold.
    pub(crate) idle: u32,
    /// Episodes that decided on the genuinely best option.
    pub(crate) correct: u32,
    /// Turns taken across the sample.
    pub(crate) turns: u64,
    /// Rounds taken across the sample: the sample's **depth**.
    ///
    /// A round's turns are authorized against the same transcript and cannot
    /// read each other, so a host with async seats waits once for all of them.
    /// `turns/ep` is width and this is depth; the two were one number until
    /// ADR 0014, which is why `vote` — one round of `budget` independent
    /// answers — was charged the same as a deliberation of `budget` rounds.
    pub(crate) rounds: u64,
    /// Calls into the library across the sample.
    pub(crate) step_calls: u64,
    /// Time spent inside the library, including an off-floor exchange
    /// round's own library time. What [`Aggregate::episodes_per_second`]
    /// divides by, since that column is the full library cost a host would
    /// pay per second, exchange included.
    pub(crate) library_time: Duration,
    /// Time spent inside `step` calls alone, excluding an exchange round's
    /// own library time. What [`Aggregate::nanos_per_step`] divides by
    /// [`Self::step_calls`], so an off-floor arm's exchange rounds do not
    /// inflate its `ns/step` against arms that never call `exchange` — see
    /// the `ns/step` glossary entry, "one call to `step`... participant time
    /// excluded".
    pub(crate) step_time: Duration,
    /// Episodes in which the room's decisive member -- its expert, or the
    /// hidden-profile member who held the deciding fact -- put its knowledge
    /// on the floor before the commit boundary.
    pub(crate) fact_deposited: u32,
    /// Episodes in which the room *had* a decisive member at all, which is
    /// the denominator `fact %` is a share of. Zero for a uniform room,
    /// where nobody holds anything anybody else does not.
    pub(crate) expert_of: u32,
    /// Sum of the turn index at which that deposit landed, over exactly the
    /// episodes counted in `fact_deposited`; the numerator for
    /// [`Aggregate::fact_latency`].
    pub(crate) fact_turns: u64,
    /// Episodes in which a [`BidReason::Knows`] bid won the floor at least
    /// once. Only an arm that folds a directory can ever populate this; the
    /// mechanism is unreachable without one.
    ///
    /// [`BidReason::Knows`]: tinyhivemind_hive::BidReason::Knows
    pub(crate) knows: u32,
    /// Turns spent on `!defer` across the sample.
    pub(crate) defers: u64,
    /// Episodes in which the decisive member also authored the first
    /// `!propose` for the topic the room went on to decide.
    pub(crate) expert_proposed: u32,
    /// Episodes in which the responder ladder selected the room's decisive
    /// member as its one responder. Only a `ladder` arm can ever populate
    /// this; every other arm leaves it at `0`.
    pub(crate) routed_right: u32,
    /// Episodes in which routing *could* be scored at all: the room named an
    /// expert on the topic the answer turns on, so there was a right answer
    /// for the ladder to have routed to. Zero for a uniform room, where
    /// nobody holds anything anybody else does not.
    pub(crate) routed_of: u32,
    /// Total cost, in [`crate::run::Participant::cost_unit`] units, spent
    /// across the sample.
    pub(crate) cost_units: u64,
    /// Model calls made in off-floor exchange rounds, summed across the
    /// sample.
    ///
    /// Each one is a model call the room paid for and the deliberation's turn
    /// count does not show. The spec for the mechanism requires it be
    /// displayed rather than folded into `cost/ep`, which is defined as each
    /// speaker's own cost times its turns and would stop meaning that.
    pub(crate) contacts: u64,
    /// Sum, in thousandths, of each episode's rank correlation between a
    /// member's folded directory weight and the number of turns it took --
    /// see [`Aggregate::mean_rho`].
    pub(crate) rank_rho_milli: i64,
    /// Episodes over which `rank_rho_milli` was accumulated: fewer than two
    /// members leaves an episode's correlation undefined, so it is skipped
    /// rather than counted as zero.
    pub(crate) rank_rho_count: u32,
    /// Whether each episode decided correctly, in the order episodes were
    /// folded in. This is the paired sample [`paired_bootstrap`] resamples
    /// against another arm's, so two arms decided over the same rooms line
    /// up index for index.
    pub(crate) correct_flags: Vec<bool>,
}

impl Aggregate {
    /// Fold another sample's totals in, as if its episodes had been added to
    /// this one in the order they were added to `other`.
    ///
    /// This is what lets the per-room loops run across cores: a worker folds
    /// the rooms it was given into totals of its own, and the parent merges
    /// those totals **in room order**. `correct_flags` is appended rather than
    /// combined, which is the whole reason the merge order matters —
    /// [`paired_bootstrap`] resamples two arms index-for-index because they
    /// decided the *same* rooms, so a merge out of order would leave the
    /// accuracy column right and every confidence interval quietly wrong.
    ///
    /// Every field is listed explicitly. `merging_matches_folding_in_one_pass`
    /// in `test.rs` is what keeps a field added later from being silently
    /// dropped here.
    pub(crate) fn merge(&mut self, other: &Self) {
        self.episodes = self.episodes.saturating_add(other.episodes);
        self.converged = self.converged.saturating_add(other.converged);
        self.deadlocked = self.deadlocked.saturating_add(other.deadlocked);
        self.exhausted = self.exhausted.saturating_add(other.exhausted);
        self.idle = self.idle.saturating_add(other.idle);
        self.correct = self.correct.saturating_add(other.correct);
        self.turns = self.turns.saturating_add(other.turns);
        self.rounds = self.rounds.saturating_add(other.rounds);
        self.step_calls = self.step_calls.saturating_add(other.step_calls);
        self.library_time = self.library_time.saturating_add(other.library_time);
        self.step_time = self.step_time.saturating_add(other.step_time);
        self.fact_deposited = self.fact_deposited.saturating_add(other.fact_deposited);
        self.expert_of = self.expert_of.saturating_add(other.expert_of);
        self.fact_turns = self.fact_turns.saturating_add(other.fact_turns);
        self.knows = self.knows.saturating_add(other.knows);
        self.defers = self.defers.saturating_add(other.defers);
        self.expert_proposed = self.expert_proposed.saturating_add(other.expert_proposed);
        self.routed_right = self.routed_right.saturating_add(other.routed_right);
        self.routed_of = self.routed_of.saturating_add(other.routed_of);
        self.cost_units = self.cost_units.saturating_add(other.cost_units);
        self.contacts = self.contacts.saturating_add(other.contacts);
        self.rank_rho_milli = self.rank_rho_milli.saturating_add(other.rank_rho_milli);
        self.rank_rho_count = self.rank_rho_count.saturating_add(other.rank_rho_count);
        self.correct_flags.extend_from_slice(&other.correct_flags);
        // Two halves of one sample, priced apart and folded together, must
        // have been priced the same way or the sum is meaningless. Every
        // aggregate in a run is built from the same `Options`, so a mismatch
        // is a harness bug rather than an operator's mistake -- hence a debug
        // assertion rather than a `Result` the callers would all have to
        // thread.
        debug_assert_eq!(
            self.model, other.model,
            "folded two samples priced against different cost models"
        );
        self.cost.merge(other.cost);
    }

    /// Fold one episode in.
    pub(crate) fn add(&mut self, report: &EpisodeReport) {
        self.episodes = self.episodes.saturating_add(1);
        match report.ending {
            Ending::Converged => self.converged = self.converged.saturating_add(1),
            Ending::Deadlocked => self.deadlocked = self.deadlocked.saturating_add(1),
            Ending::Exhausted => self.exhausted = self.exhausted.saturating_add(1),
            Ending::Idle => self.idle = self.idle.saturating_add(1),
        }
        if report.correct {
            self.correct = self.correct.saturating_add(1);
        }
        self.turns = self.turns.saturating_add(u64::from(report.turns));
        self.rounds = self.rounds.saturating_add(u64::from(report.rounds));
        self.step_calls = self.step_calls.saturating_add(u64::from(report.step_calls));
        self.library_time += report.library_time;
        self.step_time += report.step_time;
        self.correct_flags.push(report.correct);
        self.cost_units = self.cost_units.saturating_add(report.cost_units);
        self.contacts = self.contacts.saturating_add(u64::from(report.contacts));
        self.defers = self.defers.saturating_add(u64::from(report.defers));
        if report.knows_turns > 0 {
            self.knows = self.knows.saturating_add(1);
        }
        if report.has_expert {
            self.expert_of = self.expert_of.saturating_add(1);
        }
        if report.fact_deposited {
            self.fact_deposited = self.fact_deposited.saturating_add(1);
            if let Some(at) = report.fact_at {
                self.fact_turns = self.fact_turns.saturating_add(u64::from(at));
            }
        }
        if let Some(expert) = report.decisive.as_deref()
            && report.proposer.as_deref() == Some(expert)
        {
            self.expert_proposed = self.expert_proposed.saturating_add(1);
        }
        if let Some(rho) = report.rho_milli {
            self.rank_rho_milli = self.rank_rho_milli.saturating_add(rho);
            self.rank_rho_count = self.rank_rho_count.saturating_add(1);
        }
        self.cost.merge(self.model.price(&report.shape));
    }

    /// Fold one control-arm result in.
    pub(crate) fn add_arm(&mut self, report: &ArmReport) {
        self.episodes = self.episodes.saturating_add(1);
        if report.decided.is_some() {
            self.converged = self.converged.saturating_add(1);
        }
        if report.correct {
            self.correct = self.correct.saturating_add(1);
        }
        self.turns = self.turns.saturating_add(u64::from(report.turns));
        self.rounds = self.rounds.saturating_add(u64::from(report.rounds));
        self.step_calls = self.step_calls.saturating_add(1);
        self.library_time += report.library_time;
        // A control arm never opens an exchange round, so all of its library
        // time is step time.
        self.step_time += report.library_time;
        self.correct_flags.push(report.correct);
        // A control arm charges what its own speakers cost, which is one unit
        // per turn unless the room was generated with `--cost-tiers` and a
        // specialist answered.
        self.cost_units = self.cost_units.saturating_add(report.cost_units);
        if let Some(right) = report.routed_right {
            self.routed_of = self.routed_of.saturating_add(1);
            if right {
                self.routed_right = self.routed_right.saturating_add(1);
            }
        }
        self.cost.merge(self.model.price(&report.shape));
    }

    /// An empty sample that will be priced against `model`.
    ///
    /// Every aggregate in one run is built through this so that the whole
    /// table reports at one point of the cost model, and the header that
    /// prints the model describes every row under it.
    pub(crate) fn priced_at(model: crate::cost::CostModel) -> Self {
        Self {
            model,
            ..Self::default()
        }
    }

    /// Mean milliseconds a host waited for one episode: the arm's **speed**.
    pub(crate) fn latency_ms(&self) -> f64 {
        self.cost.mean_ms(self.episodes)
    }

    /// Episodes one seat pool finishes per hour: the arm's **throughput**.
    pub(crate) fn episodes_per_hour(&self) -> f64 {
        self.cost.episodes_per_hour(self.episodes)
    }

    /// Mean turns in flight: the arm's **concurrency**.
    pub(crate) fn concurrency(&self) -> f64 {
        self.cost.concurrency()
    }

    /// Mean tokens one episode spent.
    pub(crate) fn tokens_per_episode(&self) -> f64 {
        self.cost.mean_tokens(self.episodes)
    }

    /// Tokens the arm turns over per second of wall clock.
    pub(crate) fn tokens_per_second(&self) -> f64 {
        self.cost.tokens_per_second()
    }

    /// Share of episodes that decided on the best option.
    pub(crate) fn accuracy(&self) -> f64 {
        ratio(self.correct.into(), self.episodes.into()) * 100.0
    }

    /// Share of episodes that reached a recorded decision at all.
    pub(crate) fn decision_rate(&self) -> f64 {
        ratio(self.converged.into(), self.episodes.into()) * 100.0
    }

    /// Mean turns per episode: what the arm cost in **width**, and what the
    /// turn budget bounds.
    pub(crate) fn turns_per_episode(&self) -> f64 {
        ratio(self.turns, self.episodes.into())
    }

    /// Mean rounds per episode: what the arm cost in **depth**, and what a
    /// host with async seats actually waits for.
    pub(crate) fn rounds_per_episode(&self) -> f64 {
        ratio(self.rounds, self.episodes.into())
    }

    /// Mean time inside the library per call to the state machine.
    pub(crate) fn nanos_per_step(&self) -> f64 {
        let nanos = u64::try_from(self.step_time.as_nanos()).unwrap_or(u64::MAX);
        ratio(nanos, self.step_calls)
    }

    /// Episodes per second of library time, which is the throughput a host
    /// would see if its agents were free.
    pub(crate) fn episodes_per_second(&self) -> f64 {
        let seconds = self.library_time.as_secs_f64();
        if seconds <= 0.0 {
            return f64::INFINITY;
        }
        f64::from(self.episodes) / seconds
    }

    /// Share of the episodes that *had* a decisive member in which that
    /// member's knowledge reached the floor in time to matter.
    pub(crate) fn fact_reach(&self) -> f64 {
        ratio(self.fact_deposited.into(), self.expert_of.into()) * 100.0
    }

    /// Mean turn index at which that deposit landed, over the episodes in
    /// which it landed in time at all.
    pub(crate) fn fact_latency(&self) -> f64 {
        ratio(self.fact_turns, self.fact_deposited.into())
    }

    /// Share of episodes in which a `BidReason::Knows` bid won the floor at
    /// least once.
    pub(crate) fn knows_rate(&self) -> f64 {
        ratio(self.knows.into(), self.episodes.into()) * 100.0
    }

    /// Mean turns spent on `!defer` per episode.
    pub(crate) fn defers_per_episode(&self) -> f64 {
        ratio(self.defers, self.episodes.into())
    }

    /// Share of the episodes it *could* be scored over in which the
    /// responder ladder routed to the room's expert on the deciding topic.
    ///
    /// The denominator is [`Aggregate::routed_of`] rather than the whole
    /// sample: a uniform room names no expert, so an episode with nobody to
    /// route to is not a miss.
    pub(crate) fn routing_precision(&self) -> f64 {
        ratio(self.routed_right.into(), self.routed_of.into()) * 100.0
    }

    /// Share of the episodes that had a decisive member in which that member
    /// also authored the first `!propose` for the topic the room decided.
    ///
    /// The sharpest form of Q1 in `DELEGATION.md`: not merely whether the
    /// member holding the deciding knowledge got a turn, but whether the
    /// option the room settled on is the one *it* put on the floor. Printed
    /// only under `--json`, because the detail table is already at the width
    /// a terminal will hold.
    pub(crate) fn expert_led(&self) -> f64 {
        ratio(self.expert_proposed.into(), self.expert_of.into()) * 100.0
    }

    /// Mean cost, in `Participant::cost_unit` units, per episode.
    pub(crate) fn cost_per_episode(&self) -> f64 {
        ratio(self.cost_units, self.episodes.into())
    }

    /// Model calls made in off-floor exchange rounds per episode.
    ///
    /// The price of an off-floor exchange, in calls the turn count does not
    /// show. Counted as members *asked*, not rows written: a member that
    /// declines costs the same call as one that answers.
    pub(crate) fn contacts_per_episode(&self) -> f64 {
        ratio(self.contacts, self.episodes.into())
    }

    /// Correct decisions per thousand cost units spent -- a cost-normalised
    /// reading of accuracy, so an arm that spends more cannot look better
    /// than one that spends less for the same number of right answers.
    pub(crate) fn accuracy_per_kilo_unit(&self) -> f64 {
        if self.cost_units == 0 {
            return 0.0;
        }
        ratio(u64::from(self.correct) * 1000, self.cost_units)
    }

    /// Mean per-episode rank correlation between a member's folded directory
    /// weight and the number of turns it took.
    ///
    /// This is the circularity obligation `docs/specs/expert-delegation.md`
    /// writes down: a value near `1.0` says the directory reproduces the
    /// speaking order and has learned nothing except who talked, which is the
    /// failure mode `docs/research/delegation.md` names "who spoke becomes
    /// who is thought to know". A value well below `1.0` says grounded
    /// deposits and other members' citations, not turn count, decided who the
    /// directory named.
    pub(crate) fn mean_rho(&self) -> f64 {
        if self.rank_rho_count == 0 {
            return 0.0;
        }
        let sum = i32::try_from(self.rank_rho_milli).unwrap_or(i32::MAX);
        f64::from(sum) / f64::from(self.rank_rho_count)
    }
}

#[cfg(test)]
mod test;
