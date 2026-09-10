//! Measure the four endpoint-dependent constants of
//! [`crate::cost::CostModel`] against a real endpoint.
//!
//! Four of the model's five. The fifth, `tokens_per_turn`, is a property of
//! the prompt this harness sends rather than of the endpoint, and is reported
//! rather than fitted — see the closing section.
//!
//! The cost model is what turns a round shape into the tokens and milliseconds
//! every table's headline columns report, and its defaults are a guess about a
//! mid-sized hosted model. A guess is fine as long as it is *visible* and
//! *movable*, which is why every run prints the point it used and every
//! constant has a flag. This module closes the loop: it puts a handful of real
//! requests to `--api-base`, fits the four constants to what came back, and
//! prints the flags that would pin a run to the model actually being deployed.
//!
//! # How each constant is separated from the others
//!
//! Two of them come out of the token accounting the endpoint already returns,
//! and two out of the wall clock, and in both cases the separation is a
//! two-point fit rather than an assumption:
//!
//! - **`prompt_base` and `tokens_per_row`** — the same request is sent with a
//!   short transcript and a long one. Prompt tokens rise linearly in rows, so
//!   the slope between the two is the per-row cost and the intercept is
//!   everything charged before the first row.
//! - **`ttft` and `decode_rate`** — the endpoint is asked for a short answer
//!   and a long one. Latency is `ttft + completion / rate`, so again the slope
//!   is one constant and the intercept the other.
//!
//! A one-point probe cannot separate either pair, and the version of this that
//! divided total latency by total tokens reported a "decode rate" that was
//! mostly time-to-first-token — which is exactly the kind of number that then
//! gets quoted.
//!
//! # What it does not measure
//!
//! `tokens_per_turn`, the completion length. That is a property of the prompt
//! this harness sends and the protocol it asks for, not of the endpoint, and
//! the calibration reports what the probes actually produced so the operator
//! can set it from a real run rather than from here.

use std::time::Instant;

use crate::backend::http_config;
use crate::cli::Options;
use crate::cost::CostModel;
use crate::http::{Usage, ask_once};
use crate::live::AgentPrompt;
use tinyhivemind_hive::{
    BidReason, HiveTurn, Phase, QuorumPolicy, Sequence, SessionAuthor, SessionMessage, Visibility,
    aside::Audience,
};

/// Rows of synthetic transcript the short and long prompt probes carry.
///
/// Far enough apart that the difference between the two prompts dominates the
/// tokeniser's own noise on any one of them, and small enough that the long
/// probe is not itself an expensive request.
const PROBE_ROWS: (usize, usize) = (2, 40);

/// Completion lengths the short and long latency probes ask for, in tokens.
const PROBE_COMPLETION: (u32, u32) = (16, 256);

/// Times each probe is repeated.
///
/// Latency on a hosted endpoint is noisy enough that one sample of each length
/// can invert the slope outright — a slow short probe and a fast long one fit
/// a *negative* decode rate. Three of each, taking the best of each length,
/// is enough to stop that without turning a calibration into a load test.
/// Best rather than mean, deliberately: the fastest observation is the one
/// least polluted by queueing at the provider, which is not a property of the
/// model and is not what the constants are meant to carry.
const REPEATS: usize = 3;

/// Run the probes and print the fitted constants.
///
/// # Errors
///
/// Returns the backend's own error text when a probe fails, or a message when
/// the probes came back too degenerate to fit — which is a refusal to print a
/// calibration rather than an attempt to salvage one.
pub(crate) fn run(options: &Options) -> Result<(), String> {
    let config = http_config(options)?;
    let model = options.model.clone();
    println!(
        "calibrating against {} ({})\n",
        config.base.trim_end_matches('/'),
        model
    );

    let short_prompt = probe_prompt(PROBE_ROWS.0, PROBE_COMPLETION.0);
    let long_prompt = probe_prompt(PROBE_ROWS.1, PROBE_COMPLETION.0);
    let short = best_of(&config, &model, &short_prompt)?;
    let long = best_of(&config, &model, &long_prompt)?;

    // Prompt tokens against rows: slope is the per-row cost, intercept is
    // everything the endpoint charges before the first row.
    let rows = PROBE_ROWS.1.saturating_sub(PROBE_ROWS.0);
    let extra = long.usage.input.saturating_sub(short.usage.input);
    if rows == 0 || extra == 0 {
        return Err(
            "the long prompt probe was billed no more input than the short one; \
             the endpoint is not reporting usable prompt token counts"
                .to_owned(),
        );
    }
    let tokens_per_row = div_round(extra, rows as u64);
    let prompt_base = short
        .usage
        .input
        .saturating_sub(tokens_per_row.saturating_mul(PROBE_ROWS.0 as u64));

    // Latency against completion length: same fit, other pair of constants.
    let brief = best_of(
        &config,
        &model,
        &probe_prompt(PROBE_ROWS.0, PROBE_COMPLETION.0),
    )?;
    let verbose = best_of(
        &config,
        &model,
        &probe_prompt(PROBE_ROWS.0, PROBE_COMPLETION.1),
    )?;
    let more = verbose.usage.output.saturating_sub(brief.usage.output);
    let slower = verbose.ms.saturating_sub(brief.ms);
    if more == 0 || slower == 0 {
        return Err(
            "the long completion probe returned no more tokens, or no more slowly, \
             than the short one; latency cannot be split into a first-token wait \
             and a decode rate"
                .to_owned(),
        );
    }
    let decode_tok_s = div_round(more.saturating_mul(1_000), slower).max(1);
    let ttft_ms = brief
        .ms
        .saturating_sub(brief.usage.output.saturating_mul(1_000) / decode_tok_s);

    let fitted = CostModel {
        tokens_per_row: clamp_u32(tokens_per_row),
        tokens_per_turn: clamp_u32(brief.usage.output).max(1),
        prompt_base: clamp_u32(prompt_base),
        ttft_ms: clamp_u32(ttft_ms),
        decode_tok_s: clamp_u32(decode_tok_s).max(1),
    };

    println!("{}\n", fitted.header());
    println!(
        "probes: prompt {} -> {} tokens over {} -> {} rows; \
         latency {} -> {} ms over {} -> {} completion tokens",
        short.usage.input,
        long.usage.input,
        PROBE_ROWS.0,
        PROBE_ROWS.1,
        brief.ms,
        verbose.ms,
        brief.usage.output,
        verbose.usage.output,
    );
    println!(
        "\n`tokens-per-turn` above is what these probes happened to write. It is a \n\
         property of the prompt this harness sends, not of the endpoint -- take it \n\
         from a real `--api-base` run rather than from a calibration."
    );
    Ok(())
}

/// What one probe cost and how long it took.
struct Probe {
    /// Tokens the endpoint billed.
    usage: Usage,
    /// Milliseconds the request took, wall clock.
    ms: u64,
}

/// Run one probe [`REPEATS`] times and keep the fastest.
///
/// Each repeat is a single non-retrying request, so a sample either is one
/// clean request or is discarded — never the sum of a failure and its retry.
///
/// # Errors
///
/// Returns the backend's error text if every attempt failed. A probe that
/// succeeded at least once is enough, since the fastest is the one being kept
/// anyway.
fn best_of(config: &crate::http::HttpConfig, model: &str, prompt: &str) -> Result<Probe, String> {
    let mut best: Option<Probe> = None;
    let mut failure: Option<String> = None;
    for _ in 0..REPEATS {
        let started = Instant::now();
        // `ask_once`, not `ask`: a retried attempt would hand this loop two
        // requests' tokens over two requests' duration as though they were
        // one, skewing every constant fitted from it. A failed probe is
        // dropped and the next repeat stands in for it.
        match ask_once(config, model, prompt) {
            Ok(usage) => {
                let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
                let probe = Probe { usage, ms };
                if best.as_ref().is_none_or(|held| probe.ms < held.ms) {
                    best = Some(probe);
                }
            }
            Err(error) => failure = Some(error),
        }
    }
    best.ok_or_else(|| failure.unwrap_or_else(|| "every calibration probe failed".to_owned()))
}

/// A probe prompt carrying `rows` of synthetic transcript and asking for
/// roughly `completion` tokens back.
///
/// Built through [`AgentPrompt::prompt_with`] — the *real* prompt a live seat
/// is given — rather than through a synthetic preamble of this module's own.
/// That matters precisely because of what `prompt_base` is defined to be: the
/// whole non-transcript cost of a turn. A hand-written preamble omits the
/// identity line, the private-facts block, the protocol grammar, the standings
/// and the directory, so its intercept would be this module's cost rather than
/// the benchmark's, and every fitted run would underprice every turn.
///
/// The rows are uniform filler because the quantity being fitted is tokens
/// *per row*: rows of varying length would fit the mean length of this
/// harness's filler instead of the endpoint's tokenisation of a row.
fn probe_prompt(rows: usize, completion: u32) -> String {
    let seat = AgentPrompt::new(
        "planner",
        "planner",
        QuorumPolicy::DEFAULT,
        "You alone know the staged rollout was reverted once before.".to_owned(),
    );
    let turn = HiveTurn {
        agent_id: "planner".to_owned(),
        phase: Phase::Deliberate,
        visibility: Visibility::Full,
        reason: BidReason::Salience,
        watermark: Sequence(0),
        round_start: Sequence(0),
    };
    let visible: Vec<SessionMessage> = (0..rows)
        .map(|row| {
            let sequence = u64::try_from(row).unwrap_or(u64::MAX).saturating_add(1);
            SessionMessage {
                sequence: Sequence(sequence),
                author: SessionAuthor::Agent {
                    id: "alex".to_owned(),
                    label: "alex".to_owned(),
                },
                content: "!propose #stage The staged rollout limits blast radius \
                          and we can halt it at any ring."
                    .to_owned(),
                audience: Audience::Desk,
                elided: None,
            }
        })
        .collect();
    seat.prompt_with(
        &turn,
        &visible,
        &format!(
            "Write approximately {completion} tokens of plain prose summarising \
             the state of the discussion. Do not use lists."
        ),
    )
}

/// `numerator / denominator`, rounded to nearest, with a zero denominator
/// reading as zero rather than dividing.
fn div_round(numerator: u64, denominator: u64) -> u64 {
    if denominator == 0 {
        return 0;
    }
    (numerator.saturating_add(denominator / 2)) / denominator
}

/// Narrow a fitted total to the width [`CostModel`] holds it in.
fn clamp_u32(value: u64) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod test;
