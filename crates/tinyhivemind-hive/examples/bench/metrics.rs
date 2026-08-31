//! Aggregation and the tables the benchmark prints.
//!
//! Every rate is reported over the whole sample, including the episodes that
//! never decided anything: an arm cannot buy accuracy by declining to answer.

use std::time::Duration;

use crate::arms::ArmReport;
use crate::run::{Ending, EpisodeReport};

/// Running totals over a sample of episodes.
#[derive(Clone, Debug, Default)]
pub(crate) struct Aggregate {
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
    /// Calls into the library across the sample.
    pub(crate) step_calls: u64,
    /// Time spent inside the library.
    pub(crate) library_time: Duration,
}

impl Aggregate {
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
        self.step_calls = self.step_calls.saturating_add(u64::from(report.step_calls));
        self.library_time += report.library_time;
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
        self.step_calls = self.step_calls.saturating_add(1);
        self.library_time += report.library_time;
    }

    /// Share of episodes that decided on the best option.
    pub(crate) fn accuracy(&self) -> f64 {
        ratio(self.correct.into(), self.episodes.into()) * 100.0
    }

    /// Share of episodes that reached a recorded decision at all.
    pub(crate) fn decision_rate(&self) -> f64 {
        ratio(self.converged.into(), self.episodes.into()) * 100.0
    }

    /// Mean turns per episode.
    pub(crate) fn turns_per_episode(&self) -> f64 {
        ratio(self.turns, self.episodes.into())
    }

    /// Mean time inside the library per call to the state machine.
    pub(crate) fn nanos_per_step(&self) -> f64 {
        let nanos = u64::try_from(self.library_time.as_nanos()).unwrap_or(u64::MAX);
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
}

/// A safe ratio that never divides by zero.
pub(crate) fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    let numerator = u32::try_from(numerator).map_or_else(|_| lossy(numerator), f64::from);
    let denominator = u32::try_from(denominator).map_or_else(|_| lossy(denominator), f64::from);
    numerator / denominator
}

/// Widen a value too large for an exact `f64::from`, accepting the rounding
/// that only matters far above any sample size this benchmark runs.
fn lossy(value: u64) -> f64 {
    let high = f64::from(u32::try_from(value >> 32).unwrap_or(u32::MAX));
    let low = f64::from(u32::try_from(value & 0xFFFF_FFFF).unwrap_or(0));
    high.mul_add(4_294_967_296.0, low)
}

/// The header for the arm comparison table.
pub(crate) fn arm_header() -> String {
    format!(
        "{:<8}{:>10}{:>12}{:>12}{:>14}{:>14}",
        "arm", "turns/ep", "decided %", "correct %", "ns/step", "episodes/s"
    )
}

/// One row of the arm comparison table.
pub(crate) fn arm_row(name: &str, totals: &Aggregate) -> String {
    format!(
        "{:<8}{:>10.2}{:>12.1}{:>12.1}{:>14.0}{:>14.0}",
        name,
        totals.turns_per_episode(),
        totals.decision_rate(),
        totals.accuracy(),
        totals.nanos_per_step(),
        totals.episodes_per_second(),
    )
}
