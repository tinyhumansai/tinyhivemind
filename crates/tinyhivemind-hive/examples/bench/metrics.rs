//! Aggregation and the tables the benchmark prints.
//!
//! Every rate is reported over the whole sample, including the episodes that
//! never decided anything: an arm cannot buy accuracy by declining to answer.

use std::fmt::Write as _;
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
        self.correct_flags.push(report.correct);
        self.cost_units = self.cost_units.saturating_add(report.cost_units);
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

/// Width of the arm-name column in both tables.
const NAME_WIDTH: usize = 8;

/// Join an arm's name to an already-formatted row of columns.
///
/// A name longer than [`NAME_WIDTH`] eats into the leading whitespace of the
/// column beside it rather than shoving every column right, so a row for
/// `hive+defer` still lines up with a row for `hive`. A name that fits
/// produces exactly what a plain `{:<8}` would, which is what keeps the
/// published six-row table byte-identical.
fn row(name: &str, rest: &str) -> String {
    let mut over = name.chars().count().saturating_sub(NAME_WIDTH);
    let mut trimmed = rest;
    // Never eats the last space: a name flush against its first number reads
    // as one token, which is worse than a column one place out.
    while over > 0 && trimmed.starts_with("  ") {
        trimmed = trimmed.get(1..).unwrap_or(trimmed);
        over -= 1;
    }
    format!("{name:<NAME_WIDTH$}{trimmed}")
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
    let rest = format!(
        "{:>10.2}{:>12.1}{:>12.1}{:>14.0}{:>14.0}",
        totals.turns_per_episode(),
        totals.decision_rate(),
        totals.accuracy(),
        totals.nanos_per_step(),
        totals.episodes_per_second(),
    );
    row(name, &rest)
}

/// Whether `name` is an arm that actually deliberates, rather than a
/// matched-budget control.
///
/// The expert-tracking and directory-circularity fields are folded from
/// [`EpisodeReport`] by [`Aggregate::add`], which only a deliberation arm
/// ever calls; `vote` and `ladder` are folded through [`Aggregate::add_arm`]
/// instead and never populate them. [`detail_row`] uses this to print `—`
/// for a column an arm structurally cannot have data for, rather than the
/// misleading `0.0` a bare zeroed counter would otherwise print.
fn deliberates(name: &str) -> bool {
    !matches!(name, "vote" | "ladder")
}

/// The header for the statistics table: accuracy with its confidence
/// interval, the expert-delegation and cost columns, and the
/// directory-circularity proxy.
pub(crate) fn detail_header() -> String {
    format!(
        "{:<8}{:>11}{:>16}{:>8}{:>9}{:>9}{:>11}{:>9}{:>9}{:>7}",
        "arm",
        "correct %",
        "95% CI",
        "fact %",
        "to-fact",
        "knows %",
        "defers/ep",
        "route %",
        "cost/ep",
        "rho",
    )
}

/// One row of the statistics table.
///
/// `—` stands in for a column that does not apply to this arm at all: the
/// expert and rho columns for a control arm that never deliberates, and the
/// route column for every arm but a `ladder` one, which is the only kind
/// that ever consults the responder ladder.
pub(crate) fn detail_row(name: &str, totals: &Aggregate) -> String {
    let (low, high) = wilson(totals.correct, totals.episodes);
    let ci = format!("{low:.1}–{high:.1}");
    let hive_like = deliberates(name);
    let fact_pct = dash_unless(hive_like && totals.expert_of > 0, || totals.fact_reach());
    let to_fact = dash_unless(hive_like && totals.fact_deposited > 0, || {
        totals.fact_latency()
    });
    let knows_pct = dash_unless(hive_like, || totals.knows_rate());
    let defers = dash_unless(hive_like, || totals.defers_per_episode());
    // Only a ladder arm consults the responder ladder at all, and only a
    // room that names an expert on the deciding topic gives it something to
    // be right or wrong about -- so a uniform room prints `—` rather than a
    // `0.0` that would read as "the ladder always missed".
    let route_pct = dash_unless(totals.routed_of > 0, || totals.routing_precision());
    // Two decimals, not one: the number is folded in thousandths and every
    // deliberating arm lands in the same tenth, so a single decimal printed
    // `0.7` for every arm of every run and read as a constant the harness had
    // hard-coded.
    let rho = dash_unless_2(hive_like && totals.rank_rho_count > 0, || {
        totals.mean_rho() / 1000.0
    });
    let rest = format!(
        "{:>11.1}{:>16}{:>8}{:>9}{:>9}{:>11}{:>9}{:>9.2}{:>7}",
        totals.accuracy(),
        ci,
        fact_pct,
        to_fact,
        knows_pct,
        defers,
        route_pct,
        totals.cost_per_episode(),
        rho,
    );
    row(name, &rest)
}

/// Format a value to one decimal place when `available`, or `—` otherwise.
///
/// `value` is a closure rather than a plain `f64` so a caller never has to
/// compute a ratio whose denominator it already knows is zero.
fn dash_unless(available: bool, value: impl FnOnce() -> f64) -> String {
    if available {
        format!("{:.1}", value())
    } else {
        "—".to_owned()
    }
}

/// [`dash_unless`], to two decimal places.
fn dash_unless_2(available: bool, value: impl FnOnce() -> f64) -> String {
    if available {
        format!("{:.2}", value())
    } else {
        "—".to_owned()
    }
}

/// A paired-bootstrap comparison line for one deliberating arm against the
/// `vote` control, e.g. `hive+ − vote: +3.6 [+2.1, +5.0]`.
///
/// Returns `None` when the two arms were not run over the same number of
/// episodes, which is the one precondition [`paired_bootstrap`] needs to
/// treat the two flag vectors as paired samples over the same rooms.
pub(crate) fn paired_diff_line(
    name: &str,
    treatment: &Aggregate,
    control: &Aggregate,
    seed: u64,
    resamples: u32,
) -> Option<String> {
    if treatment.correct_flags.is_empty()
        || treatment.correct_flags.len() != control.correct_flags.len()
    {
        return None;
    }
    let diff = treatment.accuracy() - control.accuracy();
    let (low, high) = paired_bootstrap(
        &treatment.correct_flags,
        &control.correct_flags,
        seed,
        resamples,
    );
    Some(format!("{name} − vote: {diff:+.1} [{low:+.1}, {high:+.1}]"))
}

/// One flat JSON object for `name`, covering every column of both tables.
///
/// Hand-written with [`write!`] rather than a serialization crate: the
/// workspace does not depend on `serde_json` here and one flat object with a
/// dozen known fields does not need one. A value that is not finite --
/// [`Aggregate::episodes_per_second`] when no library time was spent, chiefly
/// -- is written as JSON `null` rather than `inf`, which is not valid JSON.
pub(crate) fn json_line(name: &str, totals: &Aggregate) -> String {
    let (low, high) = wilson(totals.correct, totals.episodes);
    let hive_like = deliberates(name);
    let mut line = String::new();
    // `write!` into a `String` never fails, so the result is discarded
    // rather than propagated.
    let _ = write!(
        line,
        "{{\"arm\":\"{name}\",\"turns_per_episode\":{},\"decision_rate\":{},\
         \"correct_pct\":{},\"ci_low\":{},\"ci_high\":{},\"ns_per_step\":{},\
         \"episodes_per_second\":{},\"fact_pct\":{},\"to_fact\":{},\
         \"knows_pct\":{},\"defers_per_episode\":{},\
         \"expert_led\":{},\"route_pct\":{},\"cost_per_episode\":{},\
         \"accuracy_per_kilo_unit\":{},\"rho\":{}}}",
        json_f64(totals.turns_per_episode()),
        json_f64(totals.decision_rate()),
        json_f64(totals.accuracy()),
        json_f64(low),
        json_f64(high),
        json_f64(totals.nanos_per_step()),
        json_f64(totals.episodes_per_second()),
        json_f64_if(hive_like && totals.expert_of > 0, totals.fact_reach()),
        json_f64_if(
            hive_like && totals.fact_deposited > 0,
            totals.fact_latency()
        ),
        json_f64_if(hive_like, totals.knows_rate()),
        json_f64_if(hive_like, totals.defers_per_episode()),
        json_f64_if(hive_like && totals.expert_of > 0, totals.expert_led()),
        json_f64_if(totals.routed_of > 0, totals.routing_precision()),
        json_f64(totals.cost_per_episode()),
        json_f64(totals.accuracy_per_kilo_unit()),
        json_f64_if(
            hive_like && totals.rank_rho_count > 0,
            totals.mean_rho() / 1000.0
        ),
    );
    line
}

/// Render a float as a JSON number, or `null` when it is not finite.
fn json_f64(value: f64) -> String {
    if value.is_finite() {
        format!("{value:.4}")
    } else {
        "null".to_owned()
    }
}

/// [`json_f64`], or `null` outright when the column does not apply.
fn json_f64_if(available: bool, value: f64) -> String {
    if available {
        json_f64(value)
    } else {
        "null".to_owned()
    }
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
    // The two-sided 97.5th percentile of the standard normal, to four places.
    const Z: f64 = 1.96;
    if trials == 0 {
        return (0.0, 0.0);
    }
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
    let bound = u32::try_from(n).unwrap_or(u32::MAX);
    let denominator = u64::try_from(n).unwrap_or(u64::MAX);
    let mut differences = Vec::with_capacity(usize::try_from(resamples).unwrap_or(0));
    let mut rng = Rng::seeded(seed);
    for _ in 0..resamples {
        let mut a_hits = 0_u32;
        let mut b_hits = 0_u32;
        for _ in 0..n {
            let index = usize::try_from(rng.below(bound)).unwrap_or(0).min(n - 1);
            if a[index] {
                a_hits = a_hits.saturating_add(1);
            }
            if b[index] {
                b_hits = b_hits.saturating_add(1);
            }
        }
        let difference = ratio(a_hits.into(), denominator) - ratio(b_hits.into(), denominator);
        differences.push(difference * 100.0);
    }
    differences.sort_by(f64::total_cmp);
    (percentile(&differences, 25), percentile(&differences, 975))
}

/// Read a percentile out of an already-sorted sample, by linear
/// interpolation between the two nearest ranks.
///
/// `per_mille` is the percentile scaled by ten (`25` for the 2.5th, `975` for
/// the 97.5th), so the rank arithmetic stays in integers up to the final
/// interpolation weight and no float is ever truncated back into an index.
fn percentile(sorted: &[f64], per_mille: u32) -> f64 {
    let Some(last) = sorted.len().checked_sub(1) else {
        return 0.0;
    };
    if last == 0 {
        return sorted[0];
    }
    let last = u64::try_from(last).unwrap_or(u64::MAX);
    let numerator = u64::from(per_mille) * last;
    let lower = usize::try_from(numerator / 1000).unwrap_or(0);
    let remainder = numerator % 1000;
    let upper = (lower + 1).min(sorted.len() - 1);
    if remainder == 0 || lower == upper {
        return sorted[lower];
    }
    let weight = f64::from(u32::try_from(remainder).unwrap_or(0)) / 1000.0;
    sorted[lower] + (sorted[upper] - sorted[lower]) * weight
}

/// Spearman's rank correlation, in thousandths (so `1000` is a perfect
/// increasing correlation and `-1000` a perfect decreasing one).
///
/// Computed as Pearson's correlation *on the ranks*, which is Spearman's
/// definition and is exact whether or not there are ties. The familiar
/// shortcut `ρ = 1 - 6·Σd²/(n·(n²-1))` is only equal to it when every rank is
/// distinct; with tied ranks it biases the magnitude toward zero, and this
/// harness ranks small vectors of directory weights and turn counts where ties
/// are the common case rather than the exception.
///
/// Ranks are computed on *doubled* averages: a tie group spanning 1-based
/// ranks `first..=last` gets the doubled rank `first + last`, which is always
/// an integer because it is a sum of two integers rather than their average.
/// Their mean is then exactly `n + 1`, another integer, so every deviation,
/// covariance and variance below stays exact integer arithmetic — the doubling
/// cancels between numerator and denominator, so no rescaling is needed:
///
/// ```text
/// a = 2·rank(x) - (n + 1)     b = 2·rank(y) - (n + 1)
/// ρ (in thousandths) = 1000·Σab / √(Σa²·Σb²)
/// ```
///
/// The one inexact step is that final square root, taken as an integer square
/// root on the squared numerator and rounded to nearest, so the result is
/// deterministic and never off by more than one thousandth.
///
/// `n` is the number of paired observations, which in this harness is always
/// a small count of arms or of policy grid points (`2..=8`); `x` and `y` must
/// be the same length. Returns `0` for fewer than two pairs, where rank
/// correlation is undefined, and for a constant vector, whose variance is zero
/// and whose correlation with anything is therefore undefined too.
pub(crate) fn spearman_milli(x: &[u32], y: &[u32]) -> i64 {
    let n = x.len();
    if n < 2 || y.len() != n {
        return 0;
    }
    // The mean of the doubled ranks is exactly `n + 1`, so centring them
    // needs no division and loses nothing.
    let mean = i64::try_from(n).unwrap_or(i64::MAX) + 1;
    let ax: Vec<i64> = doubled_ranks(x).into_iter().map(|r| r - mean).collect();
    let ay: Vec<i64> = doubled_ranks(y).into_iter().map(|r| r - mean).collect();

    let covariance: i128 = ax
        .iter()
        .zip(ay.iter())
        .map(|(a, b)| i128::from(*a) * i128::from(*b))
        .sum();
    let variance = |values: &[i64]| -> i128 {
        values
            .iter()
            .map(|value| i128::from(*value) * i128::from(*value))
            .sum()
    };
    // A vector with no spread at all — every observation tied — has no
    // ranking to correlate, and the formula would divide by zero.
    let spread = variance(&ax) * variance(&ay);
    if spread == 0 {
        return 0;
    }

    // `|ρ|·1000 = √((1000·Σab)² / Σa²Σb²)`, taken as an integer square root of
    // four times the quotient so that halving it rounds to the nearest
    // thousandth rather than always down.
    let scaled = 1000_i128 * covariance.abs();
    let quadrupled = u128::try_from(4 * scaled * scaled / spread).unwrap_or(0);
    let magnitude = i64::try_from((quadrupled.isqrt() + 1) / 2)
        .unwrap_or(1000)
        .min(1000);
    if covariance < 0 {
        -magnitude
    } else {
        magnitude
    }
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
        let first = i64::try_from(position + 1).unwrap_or(i64::MAX);
        let last = i64::try_from(end + 1).unwrap_or(i64::MAX);
        let doubled_rank = first + last;
        for slot in order.iter().take(end + 1).skip(position) {
            doubled[*slot] = doubled_rank;
        }
        position = end + 1;
    }
    doubled
}
