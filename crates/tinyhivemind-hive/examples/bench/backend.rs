//! Backend and seat configuration shared by the live drivers.
//!
//! `live_single.rs` and `live_swarm.rs` both seat scenario members over
//! either a CLI agent process or the HTTP backend, and both need the same
//! answers about which model or command a given seat runs under, what the
//! independent poll's matched backend is, and what to print once the tokens
//! come back. Keeping that surface in one module means the two drivers agree
//! on it by construction rather than by convention.

use crate::cli::Options;
use crate::http::HttpConfig;
use crate::live::Backend;
use crate::scenario::ScenarioAgent;

/// Read the HTTP backend's API key from `--api-key-env`.
///
/// # Errors
///
/// Returns a message naming the environment variable, when it is unset.
fn api_key(options: &Options) -> Result<String, String> {
    std::env::var(&options.api_key_env)
        .map_err(|_| format!("{} must be set to use --api-base", options.api_key_env))
}

/// The endpoint and credentials every HTTP seat and the HTTP poll share.
///
/// # Errors
///
/// Returns a message when no `--api-base` was given, or the key env is unset.
pub(crate) fn http_config(options: &Options) -> Result<HttpConfig, String> {
    let base = options
        .api_base
        .clone()
        .ok_or_else(|| "no --api-base given".to_owned())?;
    Ok(HttpConfig {
        base,
        key: api_key(options)?,
        wire: options.wire,
        timeout_secs: options.timeout,
        thinking: options.thinking,
    })
}

/// A short label for whichever backend is driving this run, for the banner.
pub(crate) fn backend_label(options: &Options) -> String {
    match &options.api_base {
        Some(base) => format!("{base} (model {}, {:?} wire)", options.model, options.wire),
        None => options.agent.clone().unwrap_or_default(),
    }
}

/// Whether the scenario marks `agent` as a specialist `--specialist-model`
/// should seat instead of the backend's default model.
///
/// True only for a member the scenario names as a reasoning-tier seat.
/// `expert_on:` is deliberately not enough: it names the areas a member's
/// prompt says it owns, and every seat in a hidden profile owns *some* area,
/// so reading it here seated the whole room on the specialist model — the
/// first mixed-tier live row measured an all-reasoning room by mistake. A
/// seat's model is a `tier:` claim the scenario makes on purpose.
fn is_specialist(agent: &ScenarioAgent) -> bool {
    agent.tier.as_deref() == Some("reasoning")
}

/// The model an HTTP seat for `agent` should run under.
///
/// A `--seat-model` override wins outright; otherwise a specialist seat runs
/// under `--specialist-model` when one was given, and every other seat runs
/// under the backend's default `--model`.
pub(crate) fn seat_model<'a>(options: &'a Options, agent: &ScenarioAgent) -> &'a str {
    if let Some((_, model)) = options.seat_model.iter().find(|(id, _)| *id == agent.id) {
        return model;
    }
    if is_specialist(agent)
        && let Some(model) = &options.specialist_model
    {
        return model;
    }
    &options.model
}

/// The CLI command a seat for `agent` should run.
///
/// # Errors
///
/// Returns a message when neither `--seat-cmd` nor `--agent-cmd` names one.
pub(crate) fn seat_command<'a>(
    options: &'a Options,
    agent: &ScenarioAgent,
) -> Result<&'a str, String> {
    if let Some((_, command)) = options.seat_cmd.iter().find(|(id, _)| *id == agent.id) {
        return Ok(command);
    }
    options
        .agent
        .as_deref()
        .ok_or_else(|| "no --agent-cmd or --seat-cmd names a command for this seat".to_owned())
}

/// The backend the independent poll should run under.
///
/// It has to be the same one the seats themselves ran under, or the control
/// would not be matched: an HTTP seat and a CLI seat cost different things and
/// fail under different failure modes.
///
/// # Errors
///
/// Returns a message when neither backend can be built.
pub(crate) fn poll_backend(options: &Options) -> Result<Backend, String> {
    if options.api_base.is_some() {
        return Ok(Backend::Http {
            config: http_config(options)?,
        });
    }
    if options.agent.is_none() {
        return Err("no --agent-cmd or --api-base given".to_owned());
    }
    Ok(Backend::Cli)
}

/// One HTTP seat's usage, kept alongside its boxed participant so it can be
/// read back after the episode has finished driving.
pub(crate) struct SeatUsage {
    /// The seat's canonical agent id.
    pub(crate) agent_id: String,
    /// The model this seat ran under.
    pub(crate) model: String,
    /// The tier the scenario said this seat stands in for, if it said.
    pub(crate) tier: Option<String>,
    /// The shared handle onto its running total.
    pub(crate) handle: crate::http::UsageHandle,
}

/// Cost per 1000 tokens for `model`, from `--model-cost`, or 1 by default.
fn model_cost(options: &Options, model: &str) -> u64 {
    options
        .model_cost
        .iter()
        .find(|(name, _)| name == model)
        .map_or(1, |(_, cost)| *cost)
}

/// Print each HTTP seat's tokens and cost, then the same totalled by the
/// tier the scenario put each seat on, then the arm's total.
///
/// The per-tier line is what a cost-tier claim has to be argued from: a room
/// that spends one expensive seat on the member holding the deciding fact and
/// four cheap ones elsewhere is only interesting if the split is visible.
/// Seats the scenario gave no `tier:` are totalled under `untiered`.
///
/// A room seated entirely through the CLI backend carries no [`SeatUsage`]
/// and prints nothing, since a CLI process reports no token usage to sum.
pub(crate) fn print_usage(options: &Options, seats: &[SeatUsage]) {
    if seats.is_empty() {
        return;
    }
    let mut total_tokens = 0_u64;
    let mut total_cost = 0_u64;
    for seat in seats {
        let Ok(usage) = seat.handle.lock() else {
            continue;
        };
        let usage = *usage;
        let cost = usage.cost(model_cost(options, &seat.model));
        println!(
            "usage  {:>10} model {:<10} {:>6} in  {:>6} out  {:>3} calls  cost {cost}",
            seat.agent_id, seat.model, usage.input, usage.output, usage.calls,
        );
        total_tokens = total_tokens.saturating_add(usage.tokens());
        total_cost = total_cost.saturating_add(cost);
    }
    let mut tiers: Vec<(&str, u64, u64)> = Vec::new();
    for seat in seats {
        let Ok(usage) = seat.handle.lock() else {
            continue;
        };
        let usage = *usage;
        let cost = usage.cost(model_cost(options, &seat.model));
        let tier = seat.tier.as_deref().unwrap_or("untiered");
        match tiers.iter_mut().find(|(name, _, _)| *name == tier) {
            Some(entry) => {
                entry.1 = entry.1.saturating_add(usage.tokens());
                entry.2 = entry.2.saturating_add(cost);
            }
            None => tiers.push((tier, usage.tokens(), cost)),
        }
    }
    for (tier, tokens, cost) in &tiers {
        println!("usage  tier {tier:<10} {tokens:>7} tokens  cost {cost}");
    }
    println!("usage  total {total_tokens} tokens, cost {total_cost}\n");
}
