//! Aggregation and the tables the benchmark prints.
//!
//! Every rate is reported over the whole sample, including the episodes that
//! never decided anything: an arm cannot buy accuracy by declining to answer.

use std::time::Duration;

use crate::arms::ArmReport;
use crate::rng::Rng;
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

/// A 95% Wilson score interval for a binomial rate, as percentages.
///
/// The plain `p ± 1.96·√(p(1-p)/n)` interval is wrong exactly where this
/// benchmark needs it most: near 0% or 100%, where several arms in the
/// evidential and refutation runs land, it can cross below zero or above one
/// hundred and its coverage is worst exactly there. Wilson's interval inverts
/// the normal approximation to the binomial test instead of centering on the
/// observed proportion, which keeps it inside `[0, 100]` and keeps its
/// coverage close to nominal at the sample sizes (a few hundred to a few
/// thousand episodes) this harness actually runs.
///
/// `f64` here is display-only: the returned bounds are printed in a table and
/// enter no ordering, no policy comparison, and no control flow anywhere in
/// the harness.
pub(crate) fn wilson(successes: u32, trials: u32) -> (f64, f64) {
    if trials == 0 {
        return (0.0, 0.0);
    }
    // The two-sided 97.5th percentile of the standard normal, to four places.
    const Z: f64 = 1.96;
    let n = f64::from(trials);
    let p = f64::from(successes) / n;
    let z2 = Z * Z;
    let denominator = 1.0 + z2 / n;
    let center = p + z2 / (2.0 * n);
    let spread = Z * ((p * (1.0 - p) / n) + z2 / (4.0 * n * n)).sqrt();
    let low = ((center - spread) / denominator * 100.0).clamp(0.0, 100.0);
    let high = ((center + spread) / denominator * 100.0).clamp(0.0, 100.0);
    (low, high)
}

/// A paired bootstrap interval for the mean difference in percentage points
/// between two same-length flags of per-episode correctness.
///
/// The two arms decide the *same rooms*, so their per-episode results are
/// paired rather than independent samples: resampling has to draw one shared
/// episode index for both arms on every draw, not an independent index into
/// each. Reusing the harness's own [`Rng`] rather than reaching for a crate
/// keeps the resample reproducible under `--seed` the same way every other
/// number here is.
///
/// Returns `(0.0, 0.0)` when the arrays are empty, of different lengths, or
/// identical (no resample can ever produce a nonzero difference at every
/// draw, and the mismatched-length case is a caller error this harness has no
/// way to report, so it is reported as "no evidence of a difference" rather
/// than by panicking).
pub(crate) fn paired_bootstrap(a: &[bool], b: &[bool], seed: u64, resamples: u32) -> (f64, f64) {
    let n = a.len();
    if n == 0 || a.len() != b.len() || resamples == 0 {
        return (0.0, 0.0);
    }
    let mut differences = Vec::with_capacity(resamples as usize);
    let mut rng = Rng::seeded(seed);
    for _ in 0..resamples {
        let mut a_hits = 0_u32;
        let mut b_hits = 0_u32;
        for _ in 0..n {
            let index = usize::try_from(rng.below(u32::try_from(n).unwrap_or(u32::MAX)))
                .unwrap_or(0)
                .min(n - 1);
            if a[index] {
                a_hits = a_hits.saturating_add(1);
            }
            if b[index] {
                b_hits = b_hits.saturating_add(1);
            }
        }
        let difference = ratio(a_hits.into(), n as u64) - ratio(b_hits.into(), n as u64);
        differences.push(difference * 100.0);
    }
    differences.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    (percentile(&differences, 2.5), percentile(&differences, 97.5))
}

/// Read a percentile out of an already-sorted sample, by linear
/// interpolation between the two nearest ranks.
fn percentile(sorted: &[f64], pct: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    let rank = (pct / 100.0) * (sorted.len() - 1) as f64;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    let lower = lower.min(sorted.len() - 1);
    let upper = upper.min(sorted.len() - 1);
    if lower == upper {
        return sorted[lower];
    }
    let weight = rank - lower as f64;
    sorted[lower] + (sorted[upper] - sorted[lower]) * weight
}

/// Spearman's rank correlation, in thousandths (so `1000` is a perfect
/// increasing correlation and `-1000` a perfect decreasing one).
///
/// Ranks are computed on *doubled* averages: a tie group spanning 1-based
/// ranks `first..=last` gets the doubled rank `first + last`, which is always
/// an integer because it is a sum of two integers rather than their average.
/// Every downstream quantity — the doubled-rank differences, their squares,
/// and the final formula — then stays exact integer arithmetic with no
/// floating-point rounding anywhere in the ranking itself, which is why the
/// result is an `i64` rather than an `f64`.
///
/// The ordinary Spearman formula on plain ranks is `ρ = 1 - 6·Σd²/(n·(n²-1))`.
/// Doubling every rank doubles every difference `d`, so `d²` scales by four;
/// scaling the whole formula's numerator and denominator to compensate and
/// moving to thousandths gives the formula this function implements:
///
/// ```text
/// ρ (in thousandths) = 1000 - 6000·Σd²/(4·n·(n²-1))
/// ```
///
/// `n` is the number of paired observations, which in this harness is always
/// a small count of arms or of policy grid points (`2..=8`); `x` and `y` must
/// be the same length. Returns `0` for fewer than two pairs, where rank
/// correlation is undefined.
pub(crate) fn spearman_milli(x: &[u32], y: &[u32]) -> i64 {
    let n = x.len();
    if n < 2 || y.len() != n {
        return 0;
    }
    let rx = doubled_ranks(x);
    let ry = doubled_ranks(y);
    let sum_d2: i64 = rx
        .iter()
        .zip(ry.iter())
        .map(|(a, b)| {
            let d = a - b;
            d * d
        })
        .sum();
    let n = n as i64;
    1000 - (6000 * sum_d2) / (4 * n * (n * n - 1))
}

/// Doubled average ranks (1-based), so a tie group gets an exact integer.
fn doubled_ranks(values: &[u32]) -> Vec<i64> {
    let mut order: Vec<usize> = (0..values.len()).collect();
    order.sort_by_key(|&index| values[index]);
    let mut doubled = vec![0_i64; values.len()];
    let mut position = 0_usize;
    while position < order.len() {
        let mut end = position;
        while end + 1 < order.len() && values[order[end + 1]] == values[order[position]] {
            end += 1;
        }
        // 1-based first and last rank of the tie group.
        let first = position + 1;
        let last = end + 1;
        let doubled_rank = (first + last) as i64;
        for slot in order.iter().take(end + 1).skip(position) {
            doubled[*slot] = doubled_rank;
        }
        position = end + 1;
    }
    doubled
}
