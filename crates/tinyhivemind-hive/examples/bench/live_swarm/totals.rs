//! What a federated comparison measures, and how it prints.
//!
//! One arm's report per federation ([`FederationOutcome`]), the running totals
//! folded across a sample ([`SwarmTotals`], [`FederatedTotals`]), the fan-out
//! that produces them ([`run_federated_arms`]) and the table they render as
//! ([`tabulate`]).
//!
//! Split from the drivers in [`super`] along the seam that matters here: the
//! drivers decide *what runs* — simulated federations, live desks, one traced
//! episode — and everything in this file decides *what is counted and how it
//! is shown*. Both halves outgrew one file together, and neither reads the
//! other's internals.

use std::time::Instant;

use tinyhivemind_hive::EpisodePolicy;

use tinyhivemind_hive::referral::ReferralPolicy;

use super::swarm_referrals;
use crate::TASK;
use crate::arms;
use crate::cli::Options;
use crate::federation::Federation;
use crate::metrics::{self, Aggregate};
use crate::parallel;
use crate::run;
use crate::swarm::{AskChannel, Exchange, SwarmReport, pooled, run_swarm};

/// What every arm decided about one federation.
///
/// One federation's worth of work, so a worker can produce it without touching
/// any other federation and the parent can fold the lot in federation order.
struct FederationOutcome {
    siloed: SwarmReport,
    swarmed: SwarmReport,
    offfloor: SwarmReport,
    digested: SwarmReport,
    free: SwarmReport,
    merged: crate::arms::ArmReport,
    vote: crate::arms::ArmReport,
}

/// Every federated arm's totals over one sample of federations.
struct FederatedTotals {
    /// Desks that cannot reach each other at all.
    siloed: SwarmTotals,
    /// Desks that reach each other by spending an authorized turn on it.
    swarmed: SwarmTotals,
    /// Desks that reach each other off the floor, bounded by `--ask-cap`.
    offfloor: SwarmTotals,
    /// The same, plus one reading published to every channel per desk.
    digested: SwarmTotals,
    /// The ceiling: every desk's reading already in every member's hands.
    free: SwarmTotals,
    /// One room holding every member of every desk.
    merged: Aggregate,
    /// The matched-budget independent poll across the whole federation.
    vote: Aggregate,
}

/// Run every federated arm over the same federations.
///
/// # Errors
///
/// Returns the library's own error text from any arm.
pub(super) fn run_federated_arms(
    options: &Options,
    federations: &[Federation],
    desk_policy: &EpisodePolicy,
    merged_policy: &EpisodePolicy,
) -> Result<FederatedTotals, String> {
    // One federation per worker, folded here in federation order. Every
    // federation is generated from its own seed and shares nothing with
    // another, so this is the same fold spread across cores — see
    // `crate::parallel`.
    let decided: Vec<FederationOutcome> =
        parallel::map_in_order(federations, options.jobs, |federation| {
            Ok(FederationOutcome {
                siloed: run_swarm(
                    federation,
                    desk_policy,
                    ReferralPolicy::DEFAULT,
                    Exchange::ON_FLOOR,
                    TASK,
                    false,
                )?,
                swarmed: run_swarm(
                    federation,
                    desk_policy,
                    swarm_referrals(),
                    Exchange::ON_FLOOR,
                    TASK,
                    false,
                )?,
                // The same federation, the same referral policy, the same
                // peers — asked off the floor and bounded by `--ask-cap`. The
                // only difference from `swarm` above is who pays for the
                // question.
                offfloor: run_swarm(
                    federation,
                    desk_policy,
                    swarm_referrals(),
                    Exchange {
                        asking: AskChannel::OffFloor {
                            cap: options.ask_cap,
                        },
                        digest: 0,
                    },
                    TASK,
                    false,
                )?,
                // The same arm again with every desk publishing its reading
                // once to every channel. The question it answers is the one
                // the bounded ask cannot: a federation whose desks are wrong
                // about the same thing has no peer worth asking.
                digested: run_swarm(
                    federation,
                    desk_policy,
                    swarm_referrals(),
                    Exchange {
                        asking: AskChannel::OffFloor {
                            cap: options.ask_cap,
                        },
                        digest: options.digest,
                    },
                    TASK,
                    false,
                )?,
                free: run_swarm(
                    &pooled(federation),
                    desk_policy,
                    ReferralPolicy::DEFAULT,
                    Exchange::ON_FLOOR,
                    TASK,
                    false,
                )?,
                merged: arms::run_merged(federation, merged_policy, TASK)?,
                vote: arms::run_federated_vote(federation),
            })
        })?;

    let mut siloed = SwarmTotals::default();
    let mut swarmed = SwarmTotals::default();
    let mut offfloor = SwarmTotals::default();
    let mut digested = SwarmTotals::default();
    let mut free = SwarmTotals::default();
    let mut merged = Aggregate::default();
    let mut vote = Aggregate::default();
    for outcome in &decided {
        siloed.add(&outcome.siloed);
        swarmed.add(&outcome.swarmed);
        offfloor.add(&outcome.offfloor);
        digested.add(&outcome.digested);
        free.add(&outcome.free);
        merged.add_arm(&outcome.merged);
        vote.add_arm(&outcome.vote);
    }

    Ok(FederatedTotals {
        siloed,
        swarmed,
        offfloor,
        digested,
        free,
        merged,
        vote,
    })
}

/// Running totals over a sample of federated runs.
#[derive(Clone, Debug, Default)]
struct SwarmTotals {
    /// Federations in the sample.
    runs: u32,
    /// Federations whose desks agreed on one option.
    decided: u32,
    /// Federations that landed on the genuinely best option.
    correct: u32,
    /// Agent invocations across every desk.
    turns: u64,
    /// Referrals that left the desk that made them.
    crossings: u64,
    /// Answers that arrived after the desk that asked had finished.
    stranded: u64,
    /// Questions put to another channel off the floor, taking no turn.
    off_floor_asks: u64,
    /// Readings published federation-wide, taking no turn.
    digests: u64,
    /// Desk episodes that ended in a recorded decision.
    converged: u32,
    /// Desk episodes that tied with nobody left to break it.
    deadlocked: u32,
    /// Desk episodes that spent their budget.
    exhausted: u32,
    /// Desk episodes where nobody cleared their threshold.
    idle: u32,
    /// Calls into the library.
    step_calls: u64,
    /// Time spent inside the library.
    library_time: std::time::Duration,
}

impl SwarmTotals {
    /// Fold one federated run in.
    fn add(&mut self, report: &SwarmReport) {
        self.runs = self.runs.saturating_add(1);
        if report.decided.is_some() {
            self.decided = self.decided.saturating_add(1);
        }
        if report.correct {
            self.correct = self.correct.saturating_add(1);
        }
        self.turns = self.turns.saturating_add(u64::from(report.turns));
        self.crossings = self.crossings.saturating_add(u64::from(report.crossings));
        self.stranded = self.stranded.saturating_add(u64::from(report.stranded));
        self.off_floor_asks = self
            .off_floor_asks
            .saturating_add(u64::from(report.off_floor_asks));
        self.digests = self.digests.saturating_add(u64::from(report.digests));
        self.step_calls = self.step_calls.saturating_add(u64::from(report.step_calls));
        self.library_time += report.library_time;
        for desk in &report.desks {
            match desk.ending {
                run::Ending::Converged => self.converged = self.converged.saturating_add(1),
                run::Ending::Deadlocked => self.deadlocked = self.deadlocked.saturating_add(1),
                run::Ending::Exhausted => self.exhausted = self.exhausted.saturating_add(1),
                run::Ending::Idle => self.idle = self.idle.saturating_add(1),
            }
        }
    }

    /// One row of the comparison table.
    fn row(&self, name: &str) -> String {
        let runs = u64::from(self.runs);
        format!(
            "{name:<9} {:>7.1}% {:>7} {:>9.1} {:>10.1} {:>9.1} {:>8.1} {:>9.1}",
            metrics::ratio(u64::from(self.correct), runs) * 100.0,
            self.decided,
            metrics::ratio(self.turns, runs),
            metrics::ratio(self.crossings, runs),
            metrics::ratio(self.stranded, runs),
            metrics::ratio(self.off_floor_asks, runs),
            metrics::ratio(self.digests, runs),
        )
    }
}

/// Print the federated comparison table and the totals under it.
pub(super) fn tabulate(totals: &FederatedTotals, wall: std::time::Duration, federations: usize) {
    let FederatedTotals {
        siloed,
        swarmed,
        offfloor,
        digested,
        free,
        merged,
        vote,
    } = totals;
    println!(
        "{:<9} {:>8} {:>8} {:>9} {:>10} {:>9} {:>8} {:>9}",
        "arm", "correct", "decided", "turns", "crossings", "stranded", "asks/ep", "digests",
    );
    println!("{}", siloed.row("siloed"));
    println!("{}", swarmed.row("swarm"));
    println!("{}", offfloor.row("swarm°"));
    println!("{}", digested.row("swarm◦"));
    println!("{}", free.row("pooled"));
    println!(
        "{:<9} {:>7.1}% {:>7} {:>9.1} {:>10} {:>9} {:>8} {:>9}",
        "merged",
        merged.accuracy(),
        merged.converged,
        merged.turns_per_episode(),
        "—",
        "—",
        "—",
        "—",
    );
    println!(
        "{:<9} {:>7.1}% {:>7} {:>9.1} {:>10} {:>9} {:>8} {:>9}",
        "vote",
        vote.accuracy(),
        vote.converged,
        vote.turns_per_episode(),
        "—",
        "—",
        "—",
        "—",
    );

    println!(
        "\nsiloed desk endings: converged {} · deadlocked {} · exhausted {} · idle {}",
        siloed.converged, siloed.deadlocked, siloed.exhausted, siloed.idle,
    );
    println!(
        "swarm  desk endings: converged {} · deadlocked {} · exhausted {} · idle {}",
        swarmed.converged, swarmed.deadlocked, swarmed.exhausted, swarmed.idle,
    );
    println!(
        "swarm° desk endings: converged {} · deadlocked {} · exhausted {} · idle {}",
        offfloor.converged, offfloor.deadlocked, offfloor.exhausted, offfloor.idle,
    );
    println!(
        "swarm◦ desk endings: converged {} · deadlocked {} · exhausted {} · idle {}",
        digested.converged, digested.deadlocked, digested.exhausted, digested.idle,
    );
    println!(
        "library time {:.1} ms over {} steps ({:.0} ns/step)",
        swarmed.library_time.as_secs_f64() * 1_000.0,
        swarmed.step_calls,
        metrics::ratio(
            u64::try_from(swarmed.library_time.as_nanos()).unwrap_or(u64::MAX),
            swarmed.step_calls,
        ),
    );
    println!(
        "wall clock {:.1} ms for {} federations across every arm",
        wall.as_secs_f64() * 1_000.0,
        federations,
    );
}

