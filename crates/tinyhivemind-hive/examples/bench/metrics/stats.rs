//! The statistics under the benchmark's second table, and the arithmetic they
//! rest on.
//!
//! Two layers, in one file because the second exists only to serve the first.
//! The small pieces come first -- widening a `u64` too large for an exact
//! `f64` cast, reading a percentile out of a sorted sample, ranking a tied
//! vector -- and then the three statistics the tables actually print:
//! [`wilson`], [`paired_bootstrap`] and [`spearman_milli`].
//!
//! Those three moved here when `mod.rs` reached the file-length cap, and this
//! is the seam rather than a convenient line: `mod.rs` folds episodes into
//! totals and renders them, and none of the three does either.
//! `--stats-check` runs all three against properties they are defined to
//! have; see `main.rs`.

/// Widen a value too large for an exact `f64::from`, accepting the rounding
/// that only matters far above any sample size this benchmark runs.
pub(super) fn lossy(value: u64) -> f64 {
    let high = f64::from(u32::try_from(value >> 32).unwrap_or(u32::MAX));
    let low = f64::from(u32::try_from(value & 0xFFFF_FFFF).unwrap_or(0));
    high.mul_add(4_294_967_296.0, low)
}

/// Read a percentile out of an already-sorted sample, by linear
/// interpolation between the two nearest ranks.
///
/// `per_mille` is the percentile scaled by ten (`25` for the 2.5th, `975` for
/// the 97.5th), so the rank arithmetic stays in integers up to the final
/// interpolation weight and no float is ever truncated back into an index.
pub(super) fn percentile(sorted: &[f64], per_mille: u32) -> f64 {
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

/// Doubled average ranks (1-based), so a tie group gets an exact integer.
pub(super) fn doubled_ranks(values: &[u32]) -> Vec<i64> {
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
    let magnitude = i64::try_from(quadrupled.isqrt().div_ceil(2))
        .unwrap_or(1000)
        .min(1000);
    if covariance < 0 {
        -magnitude
    } else {
        magnitude
    }
}
