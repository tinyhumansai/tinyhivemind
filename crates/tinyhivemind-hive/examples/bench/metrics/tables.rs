//! The tables an [`Aggregate`] prints as: the six-column headline arm
//! comparison, the library-cost pair, the wide detail table, the paired
//! comparison lines a finding is reported against, and the flat `--json`
//! object covering every column above.
//!
//! Split out of [`super`] because it is read differently from the fold it
//! reads: nothing here changes what an [`Aggregate`] holds, only how one is
//! rendered, and keeping the rendering apart from the accounting is what lets
//! `super::test` cover the fold without a table's formatting noise.

use std::fmt::Write as _;

use super::Aggregate;
use super::format::{
    count, dash_unless, dash_unless_2, deliberates, duration, json_f64, json_f64_if, row,
};
use super::stats::lossy;
use super::{paired_bootstrap, wilson};

/// A safe ratio that never divides by zero.
pub(crate) fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    let numerator = u32::try_from(numerator).map_or_else(|_| lossy(numerator), f64::from);
    let denominator = u32::try_from(denominator).map_or_else(|_| lossy(denominator), f64::from);
    numerator / denominator
}

/// The header for the arm comparison table.
pub(crate) fn arm_header() -> String {
    format!(
        "{:<16}{:>10}{:>10}{:>11}{:>8}{:>10}{:>10}",
        "arm", "quality", "speed", "thru", "conc", "tok/ep", "tok/s"
    )
}

/// One row of the arm comparison table.
///
/// Six columns, the same six for every arm, and every one of them a quantity a
/// host pays or receives:
///
/// - **quality** — share of episodes that decided the genuinely best option.
/// - **speed** — mean wall clock from brief to decision, summed over rounds.
/// - **thru** — episodes one seat pool finishes per hour at that latency.
/// - **conc** — mean turns in flight, `1.0` for a strictly sequential arm.
/// - **tok/ep** — prompt and completion tokens one episode spends.
/// - **tok/s** — the rate those tokens are drawn at, which is what a provider
///   rate limit is written against.
///
/// The columns this replaced — `ns/step` and `episodes/s` — measured the
/// library rather than the system, and reading them as though they measured
/// the system is what made `vote` look infinitely fast: a poll never calls the
/// library at all. `ns/step` still exists and is still worth knowing; it moved
/// to [`library_header`], under a heading that says what it is.
pub(crate) fn arm_row(name: &str, totals: &Aggregate) -> String {
    let rest = format!(
        "{:>9.1}%{:>9}{:>11}{:>8.1}{:>10}{:>10}",
        totals.accuracy(),
        duration(totals.latency_ms()),
        count(totals.episodes_per_hour()),
        totals.concurrency(),
        count(totals.tokens_per_episode()),
        count(totals.tokens_per_second()),
    );
    row(name, &rest)
}

/// The header for what the *library* costs, as against what the system costs.
///
/// Kept apart from [`arm_header`] on purpose. These two columns are the price
/// of running the state machine with every agent's own time excluded, and they
/// are genuinely tiny — which is the finding. Printed beside token and latency
/// columns they invited the opposite reading, that an arm taking no library
/// time was free.
pub(crate) fn library_header() -> String {
    format!(
        "{:<16}{:>10}{:>10}{:>12}{:>14}",
        "arm", "turns/ep", "rounds/ep", "decided %", "ns/step"
    )
}

/// One row of the library-cost table.
pub(crate) fn library_row(name: &str, totals: &Aggregate) -> String {
    let rest = format!(
        "{:>10.2}{:>10.2}{:>12.1}{:>14.0}",
        totals.turns_per_episode(),
        totals.rounds_per_episode(),
        totals.decision_rate(),
        totals.nanos_per_step(),
    );
    row(name, &rest)
}

/// The header for the statistics table: accuracy with its confidence
/// interval, the expert-delegation and cost columns, and the
/// directory-circularity proxy.
pub(crate) fn detail_header() -> String {
    format!(
        "{:<8}{:>11}{:>16}{:>8}{:>9}{:>9}{:>11}{:>9}{:>9}{:>10}{:>7}",
        "arm",
        "correct %",
        "95% CI",
        "fact %",
        "to-fact",
        "knows %",
        "defers/ep",
        "route %",
        "cost/ep",
        "calls/ep",
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
        "{:>11.1}{:>16}{:>8}{:>9}{:>9}{:>11}{:>9}{:>9.2}{:>10}{:>7}",
        totals.accuracy(),
        ci,
        fact_pct,
        to_fact,
        knows_pct,
        defers,
        route_pct,
        totals.cost_per_episode(),
        // `—` rather than `0.0` for an arm that runs no exchange at all: the
        // column is about a mechanism most arms do not have, not a count they
        // scored zero on.
        dash_unless(totals.contacts > 0, || totals.contacts_per_episode()),
        rho,
    );
    row(name, &rest)
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

/// The same paired bootstrap, against a named control other than `vote`.
///
/// `vote` is the right control for the published table — it answers "is a room
/// worth more than a matched-budget poll". It is the wrong control for a
/// mechanism *inside* a room, where the question is whether the mechanism
/// changed anything against the same room without it.
pub(crate) fn paired_against(
    name: &str,
    control_name: &str,
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
    Some(format!(
        "{name} − {control_name}: {diff:+.1} [{low:+.1}, {high:+.1}]"
    ))
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
        "{{\"arm\":\"{name}\",\
         \"latency_ms\":{},\"episodes_per_hour\":{},\"concurrency\":{},\
         \"tokens_per_episode\":{},\"tokens_per_second\":{},\
         \"turns_per_episode\":{},\"rounds_per_episode\":{},\"decision_rate\":{},\
         \"correct_pct\":{},\"ci_low\":{},\"ci_high\":{},\"ns_per_step\":{},\
         \"episodes_per_second\":{},\"fact_pct\":{},\"to_fact\":{},\
         \"knows_pct\":{},\"defers_per_episode\":{},\
         \"expert_led\":{},\"route_pct\":{},\"cost_per_episode\":{},\
         \"accuracy_per_kilo_unit\":{},\"rho\":{},\"exchange_calls_per_episode\":{}}}",
        // The six headline `arm_row` columns -- quality (`correct_pct`,
        // below), speed, throughput, concurrency, and the two token rates --
        // so a `--json` consumer can read every column the table prints
        // rather than only the library/detail fields this line covered
        // before the headline table existed.
        json_f64(totals.latency_ms()),
        json_f64(totals.episodes_per_hour()),
        json_f64(totals.concurrency()),
        json_f64(totals.tokens_per_episode()),
        json_f64(totals.tokens_per_second()),
        json_f64(totals.turns_per_episode()),
        json_f64(totals.rounds_per_episode()),
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
        // Model calls made in off-floor exchange rounds, per episode -- the
        // same figure the `calls/ep` column reports, dashed there when no arm
        // in the run made any. Members *asked*, not rows written: a member
        // that declines costs the same call as one that answers, so counting
        // rows would report a price below the one paid. `0.0` here rather
        // than `null` when nothing was spent, matching `cost_per_episode`'s
        // zero rather than every other `hive_like`-gated field's `null`,
        // since this is a total rather than a rate only a hive-like arm can
        // attempt.
        json_f64(totals.contacts_per_episode()),
    );
    line
}
