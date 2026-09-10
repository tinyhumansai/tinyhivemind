//! Every `--<flag>` handler [`super::Options::parse`] delegates to.
//!
//! Split out of `mod.rs` because a CLI surface this wide has more matching
//! logic than declaration: the room and federation knobs, both live
//! backends, and the delegation and check-arm caps are each one flag away
//! from the next, and grouping them by what they configure — scale,
//! expertise, live backend — keeps each function under the line budget
//! clippy holds every function to, without inventing a home for anything
//! that would not otherwise have one.

use super::{Mode, Options};
use crate::http::{Thinking, Wire};
use crate::sim::Expertise;
use tinyhivemind_hive::Basis;
use tinyhivemind_hive::DirectoryPolicy;

/// Apply one of the scale flags (`--jobs`, `--ask-cap`, `--digest`,
/// `--distance`, `--evidence`) to `options`.
///
/// Returns whether the flag was one of them, on the same contract as
/// [`apply_expertise_flag`]. These two are together because they are the two
/// knobs that decide what a large run costs: how many cores it spreads over,
/// and how many questions a desk puts to other channels.
///
/// # Errors
///
/// Returns a message when `--distance` is given a value that is neither
/// `sequence` nor `live`.
pub(super) fn apply_scale_flag(
    options: &mut Options,
    flag: &str,
    args: &mut impl Iterator<Item = String>,
) -> Result<bool, String> {
    match flag {
        "--jobs" => {
            options.jobs = usize::try_from(next_number(args).unwrap_or(1))
                .unwrap_or(1)
                .max(1);
        }
        "--scenario-dir" => {
            options.scenario_dir = args.next();
            if !matches!(options.mode, Mode::Swarm) {
                options.mode = Mode::Live;
            }
        }
        "--ask-cap" => {
            options.ask_cap = usize::try_from(next_number(args).unwrap_or(2)).unwrap_or(2);
        }
        "--distance" => {
            // The one knob that changes what a *sequence* means to the
            // library's own folds: `live` counts the rows the episode reads
            // rather than every row the host wrote. Identical on a journal the
            // episode owns end to end, which is why every recorded number is
            // unchanged by leaving it alone.
            // Named exhaustively rather than defaulted. A typo would
            // otherwise select `sequence` in silence and run a different
            // benchmark under the heading the operator asked for — the same
            // failure an unrecognised *flag* is refused for, one level down,
            // and a missing value would swallow the next flag as its argument.
            options.policy.distance = match args.next().as_deref() {
                Some("sequence") => Basis::Sequence,
                Some("live") => Basis::Live,
                Some(other) => {
                    return Err(format!(
                        "--distance takes `sequence` or `live`, not `{other}`"
                    ));
                }
                None => return Err("--distance takes `sequence` or `live`".to_owned()),
            };
        }
        "--digest" => {
            options.digest = usize::try_from(next_number(args).unwrap_or(1)).unwrap_or(1);
        }
        "--evidence" => options.evidence = true,
        "--fact-noise" => {
            // Parsed strictly rather than through `next_number`'s
            // `unwrap_or(0)`: that default reads a missing or nonnumeric
            // value as "no wrong facts", which is truthful evidence rather
            // than the garbage input it actually was, and — because
            // `args.next()` still consumes the token either way — it also
            // eats whatever flag came next. `--fact-noise --swarm` would
            // silently drop `--swarm` and leave the mode unchanged. Read the
            // token and reject it explicitly instead.
            let raw = args
                .next()
                .ok_or_else(|| "--fact-noise requires a per-mille value".to_owned())?;
            let noise = raw.parse::<u32>().map_err(|_| {
                format!("--fact-noise takes a value from 0 to 1000 (per mille), not {raw}")
            })?;
            // `Federation::planted_with` reads `draws.below(1_000) < wrong`,
            // which is already `true` for every draw once `wrong` passes
            // 1000 — a value above the scale silently behaves exactly like
            // 1000 rather than the (nonsensical) "more than all of them" an
            // operator might have meant. Refused rather than clamped, on the
            // same rule an unrecognised flag is refused for: a benchmark that
            // silently measures something other than what was asked for is
            // worse than one that stops.
            if noise > 1_000 {
                return Err(format!(
                    "--fact-noise takes a value from 0 to 1000 (per mille), not {noise}"
                ));
            }
            options.fact_noise = noise;
            options.evidence = true;
        }
        _ => return Ok(false),
    }
    Ok(true)
}

/// Apply one of `--specialists`, `--hidden-profile`, `--defer-cap`,
/// `--cost-tiers`, `--blind-evidence` or `--directory` to `options`.
///
/// Returns whether the flag was one of them, so [`Options::parse`] can tell a
/// flag this harness handles somewhere from one it handles nowhere.
pub(super) fn apply_expertise_flag(
    options: &mut Options,
    flag: &str,
    args: &mut impl Iterator<Item = String>,
) -> bool {
    match flag {
        "--specialists" => {
            let count = usize::try_from(next_number(args).unwrap_or(0)).unwrap_or(0);
            options.expertise = Expertise::Specialists { count };
        }
        "--hidden-profile" => options.expertise = Expertise::HiddenProfile,
        "--defer-cap" => options.defer_cap = next_number(args).unwrap_or(1).max(1),
        "--aside-cap" => options.aside_cap = next_number(args).unwrap_or(1),
        "--round-width" => {
            options.round_width =
                next_number(args).unwrap_or(tinyhivemind_hive::DEFAULT_ROUND_WIDTH);
        }
        "--context" => options.context = next_number(args).unwrap_or(0) as usize,
        "--rot" => {
            options.rot = args
                .next()
                .and_then(|value| value.parse::<f64>().ok())
                .unwrap_or(0.0)
                .clamp(0.0, 1.0);
        }
        "--context-sweep" => options.mode = Mode::ContextSweep,
        "--scale-sweep" => options.mode = Mode::ScaleSweep,
        "--stages" => {
            options.mode = Mode::StageSweep;
            // A list sweeps a ladder of horizons; a bare number runs one. Both
            // spellings are useful: the ladder is where the crossover lives,
            // and one length is what a follow-up reproduces.
            if let Some(list) = args.next() {
                let parsed: Vec<usize> = list
                    .split(',')
                    .filter_map(|part| part.trim().parse::<usize>().ok())
                    .filter(|stages| *stages >= 1)
                    .collect();
                if !parsed.is_empty() {
                    options.horizons = parsed;
                }
            }
        }
        "--facets" => {
            options.mode = Mode::FacetSweep;
            // A list sweeps a ladder of widths; a bare number runs one. Both
            // spellings are useful, for the reason `--stages` takes both.
            if let Some(list) = args.next() {
                let parsed: Vec<usize> = list
                    .split(',')
                    .filter_map(|part| part.trim().parse::<usize>().ok())
                    .filter(|facets| *facets >= 1)
                    .collect();
                if !parsed.is_empty() {
                    options.facets = parsed;
                }
            }
        }
        "--roles" => options.roles = true,
        "--fidelity" => {
            // `f64::parse` accepts `"nan"`, `"inf"` and `"-inf"`, and
            // `f64::clamp` leaves a `NaN` exactly as it found it rather than
            // bounding it, so an unfiltered non-finite value would reach
            // `ContextBudget::worth` and poison every folded row's score.
            // Reject it the same way an unparsable value already is: leave
            // the prior value in place.
            if let Some(value) = args
                .next()
                .and_then(|raw| raw.parse::<f64>().ok())
                .filter(|value| value.is_finite())
            {
                options.fidelity = value.clamp(0.0, 1.0);
            }
        }
        "--sizes" => {
            if let Some(list) = args.next() {
                let parsed: Vec<usize> = list
                    .split(',')
                    .filter_map(|part| part.trim().parse::<usize>().ok())
                    .filter(|size| *size >= 2)
                    .collect();
                if !parsed.is_empty() {
                    options.sizes = parsed;
                }
            }
        }
        "--exchange-cap" => options.exchange_cap = next_number(args).unwrap_or(4),
        "--history" => options.history = next_number(args).unwrap_or(3),
        "--cost-tiers" => options.cost = true,
        "--blind-evidence" => options.blind_evidence = true,
        // `--trace` prints one episode at `options.policy`, so without this
        // there is no way to watch the delegation arm run: `BidReason::Knows`
        // is unreachable unless a directory is folded, and only an arm sets
        // that. It moves the same single field `knowing_policy` moves.
        "--directory" => options.policy.directory = Some(DirectoryPolicy::DEFAULT),
        _ => return false,
    }
    true
}

/// Apply one of the live-backend flags (`--timeout` through
/// `--specialist-model`) to `options`.
///
/// Returns whether the flag was one of them, on the same contract as
/// [`apply_expertise_flag`].
pub(super) fn apply_live_flag(
    options: &mut Options,
    flag: &str,
    args: &mut impl Iterator<Item = String>,
) -> bool {
    match flag {
        "--timeout" => options.timeout = u64::from(next_number(args).unwrap_or(180)),
        "--api-base" => {
            if let Some(base) = args.next() {
                options.api_base = Some(base);
                // `--swarm --api-base` drives a federation rather than one
                // room, so the swarm mode keeps the floor.
                if !matches!(options.mode, Mode::Swarm) {
                    options.mode = Mode::Live;
                }
            }
        }
        "--api-key-env" => {
            if let Some(name) = args.next() {
                options.api_key_env = name;
            }
        }
        "--model" => {
            if let Some(model) = args.next() {
                options.model = model;
            }
        }
        "--wire" => {
            if let Some(text) = args.next()
                && let Some(wire) = Wire::parse(&text)
            {
                options.wire = wire;
            }
        }
        "--model-cost" => {
            if let Some(spec) = args.next()
                && let Some((model, cost)) = spec.split_once('=')
                && let Ok(cost) = cost.parse::<u64>()
            {
                options.model_cost.push((model.to_owned(), cost));
            }
        }
        "--seat-model" => {
            if let Some(spec) = args.next()
                && let Some((agent, model)) = spec.split_once('=')
            {
                options
                    .seat_model
                    .push((agent.to_owned(), model.to_owned()));
            }
        }
        "--seat-cmd" => {
            if let Some(spec) = args.next()
                && let Some((agent, command)) = spec.split_once('=')
            {
                options
                    .seat_cmd
                    .push((agent.to_owned(), command.to_owned()));
            }
        }
        "--specialist-model" => options.specialist_model = args.next(),
        "--thinking" => {
            if let Some(text) = args.next()
                && let Some(thinking) = Thinking::parse(&text)
            {
                options.thinking = thinking;
            }
        }
        _ => return false,
    }
    true
}

/// Read the next argument as a number.
pub(super) fn next_number(args: &mut impl Iterator<Item = String>) -> Option<u32> {
    args.next()?.parse().ok()
}

/// Read one flag's number out of the whole argument list.
pub(super) fn flag_number(args: &[String], flag: &str) -> Option<u32> {
    let at = args.iter().position(|argument| argument == flag)?;
    args.get(at + 1)?.parse().ok()
}

/// Apply one of the flags that select what the run *does* rather than how it
/// is configured.
///
/// Returns whether `flag` was one of them, on the same contract as
/// [`apply_expertise_flag`]. Grouped here because they are the one family of
/// flags that are mutually exclusive in effect but not in spelling: several of
/// them are commonly given together (`--swarm --trace`), and the precedence
/// between them is a rule rather than a last-one-wins accident. Keeping that
/// rule in one function is what makes it reviewable.
///
/// The rule: `--swarm` selects a federation and keeps the floor against
/// `--trace`, which then prints a federation transcript rather than a single
/// room's. Everything else is last-one-wins.
pub(super) fn apply_mode_flag(options: &mut Options, flag: &str) -> bool {
    match flag {
        "--swarm" => options.mode = Mode::Swarm,
        "--sweep" => options.mode = Mode::Sweep,
        "--stats-check" => options.mode = Mode::StatsCheck,
        "--grid" => options.mode = Mode::Grid,
        "--calibrate" => options.mode = Mode::Calibrate,
        "--trace" => {
            options.trace = true;
            if !matches!(options.mode, Mode::Swarm) {
                options.mode = Mode::Trace;
            }
        }
        _ => return false,
    }
    true
}
