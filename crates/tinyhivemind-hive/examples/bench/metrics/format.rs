//! Formatting helpers for the printed and `--json` tables.
//!
//! Nothing here computes a statistic; each function only decides how an
//! already-computed [`super::Aggregate`] rate is rendered -- as a fixed-width
//! table cell, a dash for a column an arm structurally cannot have data for,
//! or a JSON number (or `null`).

/// Width of the arm-name column in every table.
///
/// Wide enough for the longest arm name the comparison prints
/// (`hive+dir+defer`), so no row has to fall back on [`row`]'s
/// whitespace-eating. That fallback is still there, and still guards the next
/// name somebody adds.
const NAME_WIDTH: usize = 16;

/// Join an arm's name to an already-formatted row of columns.
///
/// A name longer than [`NAME_WIDTH`] eats into the leading whitespace of the
/// column beside it rather than shoving every column right, so a row for
/// `hive+defer` still lines up with a row for `hive`. A name that fits
/// produces exactly what a plain `{:<8}` would, which is what keeps the
/// published six-row table byte-identical.
pub(super) fn row(name: &str, rest: &str) -> String {
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

/// Whether `name` is an arm that actually deliberates, rather than a
/// matched-budget control.
///
/// The expert-tracking and directory-circularity fields are folded from
/// [`crate::run::EpisodeReport`] by [`super::Aggregate::add`], which only a
/// deliberation arm ever calls; `vote` and `ladder` are folded through
/// [`super::Aggregate::add_arm`] instead and never populate them.
/// [`super::detail_row`] uses this to print `—` for a column an arm
/// structurally cannot have data for, rather than the misleading `0.0` a bare
/// zeroed counter would otherwise print.
pub(super) fn deliberates(name: &str) -> bool {
    !matches!(name, "vote" | "ladder")
}

/// Format a value to one decimal place when `available`, or `—` otherwise.
///
/// `value` is a closure rather than a plain `f64` so a caller never has to
/// compute a ratio whose denominator it already knows is zero.
pub(super) fn dash_unless(available: bool, value: impl FnOnce() -> f64) -> String {
    if available {
        format!("{:.1}", value())
    } else {
        "—".to_owned()
    }
}

/// [`dash_unless`], to two decimal places.
pub(super) fn dash_unless_2(available: bool, value: impl FnOnce() -> f64) -> String {
    if available {
        format!("{:.2}", value())
    } else {
        "—".to_owned()
    }
}

/// Render a float as a JSON number, or `null` when it is not finite.
pub(super) fn json_f64(value: f64) -> String {
    if value.is_finite() {
        format!("{value:.4}")
    } else {
        "null".to_owned()
    }
}

/// [`json_f64`], or `null` outright when the column does not apply.
pub(super) fn json_f64_if(available: bool, value: f64) -> String {
    if available {
        json_f64(value)
    } else {
        "null".to_owned()
    }
}

/// Render a mean latency in milliseconds as a human duration.
///
/// Milliseconds below a second, then seconds, then minutes. A deliberation and
/// a single routed turn differ by two orders of magnitude on this column, and
/// a table printing both as bare milliseconds makes the reader count digits to
/// see it.
pub(super) fn duration(ms: f64) -> String {
    if !ms.is_finite() || ms <= 0.0 {
        return "—".to_owned();
    }
    if ms < 1_000.0 {
        return format!("{ms:.0}ms");
    }
    let secs = ms / 1_000.0;
    if secs < 90.0 {
        return format!("{secs:.1}s");
    }
    format!("{:.1}m", secs / 60.0)
}

/// Render a rate or a count with a thousands suffix.
///
/// Same argument as [`duration`]: `tok/ep` spans a single routed turn's few
/// hundred and a wide federation's hundreds of thousands, and a column of raw
/// integers at both ends is read by counting digits rather than by looking.
pub(super) fn count(value: f64) -> String {
    if !value.is_finite() || value <= 0.0 {
        return "—".to_owned();
    }
    if value < 1_000.0 {
        return format!("{value:.0}");
    }
    if value < 1_000_000.0 {
        return format!("{:.1}k", value / 1_000.0);
    }
    format!("{:.1}M", value / 1_000_000.0)
}
