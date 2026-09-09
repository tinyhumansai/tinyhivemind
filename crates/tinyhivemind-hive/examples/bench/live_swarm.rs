//! Driving a live federation of desks through a real backend.
//!
//! `swarm_compare` is what `--swarm` selects: several desks, each seated over
//! a CLI agent or the HTTP backend, that can only reach each other through a
//! referral. Split out of `main.rs` as the federated counterpart to
//! `live_single.rs`'s single-room drivers — the two share the seat-building
//! and usage-printing surface in `backend.rs`, but differ in everything about
//! how a report is tallied and printed, since a federation's report covers
//! several desks rather than one room.

use std::time::Instant;

use tinyhivemind_hive::referral::ReferralPolicy;
use tinyhivemind_hive::{EpisodePolicy, QuorumPolicy};

use crate::TASK;
use crate::arms;
use crate::backend::{
    SeatUsage, backend_label, http_config, poll_backend, print_usage, seat_command, seat_model,
};
use crate::cli::Options;
use crate::federation::Federation;
use crate::http::{HttpAgent, HttpDeskAgent};
use crate::live::{self, AgentPrompt, LiveAgent};
use crate::live_single::{plurality, verdict};
use crate::metrics::{self, Aggregate};
use crate::parallel;
use crate::policy::{quorum_threshold, turn_budget};
use crate::rng::mix;
use crate::run;
use crate::scenario::Scenario;
use crate::swarm::{self, AskChannel, Channel, SwarmMember, SwarmReport, pooled, run_swarm};

/// What every arm decided about one federation.
///
/// One federation's worth of work, so a worker can produce it without touching
/// any other federation and the parent can fold the lot in federation order.
struct FederationOutcome {
    siloed: SwarmReport,
    swarmed: SwarmReport,
    offfloor: SwarmReport,
    free: SwarmReport,
    merged: crate::arms::ArmReport,
    vote: crate::arms::ArmReport,
}

/// The referral policy the swarm arm runs at.
///
/// Two hops is one round trip — the question out, the answer back — and it is
/// the value `OpenCompany` defaults its mention-dispatch chain to, so the swarm
/// is not quietly given a deeper chain than a host would allow. Desk mentions
/// and returns are on because they are the mechanism under test; without them
/// the arm is the siloed control with extra steps.
const fn swarm_referrals() -> ReferralPolicy {
    ReferralPolicy {
        enabled: true,
        max_hops: 2,
        reach: tinyhivemind_hive::referral::ReferralReach::Desks,
        returns: true,
    }
}

/// Run every federated arm over the same federations and print the comparison.
pub(crate) fn swarm_compare(options: &Options) -> Result<(), String> {
    if options.agent.is_some() || options.api_base.is_some() {
        let Some(path) = &options.scenario else {
            return Err("a live federation needs --scenario".to_owned());
        };
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("could not read {path}: {error}"))?;
        let scenario = Scenario::parse(&text)?;
        return live_federation(options, &scenario);
    }
    let federations: Vec<Federation> = (0..options.episodes)
        .map(|index| {
            Federation::generate(
                mix(options.seed, u64::from(index)),
                options.desks,
                options.per_desk,
                options.topics,
                options.noise,
                options.bias,
            )
        })
        .collect();
    let Some(first) = federations.first() else {
        return Err("no federations generated".to_owned());
    };

    // Each desk deliberates at the budget its own size earns, exactly as it
    // would if it were the only desk. The merged control is given the whole
    // federation's budget instead, which is more turns than any desk has.
    let desk_policy = EpisodePolicy {
        turn_budget: turn_budget(options.per_desk),
        quorum: QuorumPolicy {
            threshold: quorum_threshold(options.per_desk),
            ..options.policy.quorum
        },
        ..options.policy
    };
    let whole = first.agents.len();
    let merged_policy = EpisodePolicy {
        turn_budget: turn_budget(whole),
        quorum: QuorumPolicy {
            threshold: quorum_threshold(whole),
            ..options.policy.quorum
        },
        ..options.policy
    };

    describe(
        options,
        first,
        &desk_policy,
        &merged_policy,
        federations.len(),
    );
    if options.trace {
        trace_swarm(first, &desk_policy)?;
    }

    let wall = Instant::now();
    let totals = run_federated_arms(options, &federations, &desk_policy, &merged_policy)?;
    let wall = wall.elapsed();

    tabulate(&totals, wall, federations.len());
    Ok(())
}

/// Every federated arm's totals over one sample of federations.
struct FederatedTotals {
    /// Desks that cannot reach each other at all.
    siloed: SwarmTotals,
    /// Desks that reach each other by spending an authorized turn on it.
    swarmed: SwarmTotals,
    /// Desks that reach each other off the floor, bounded by `--ask-cap`.
    offfloor: SwarmTotals,
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
fn run_federated_arms(
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
                    AskChannel::OnFloor,
                    TASK,
                    false,
                )?,
                swarmed: run_swarm(
                    federation,
                    desk_policy,
                    swarm_referrals(),
                    AskChannel::OnFloor,
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
                    AskChannel::OffFloor {
                        cap: options.ask_cap,
                    },
                    TASK,
                    false,
                )?,
                free: run_swarm(
                    &pooled(federation),
                    desk_policy,
                    ReferralPolicy::DEFAULT,
                    AskChannel::OnFloor,
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
    let mut free = SwarmTotals::default();
    let mut merged = Aggregate::default();
    let mut vote = Aggregate::default();
    for outcome in &decided {
        siloed.add(&outcome.siloed);
        swarmed.add(&outcome.swarmed);
        offfloor.add(&outcome.offfloor);
        free.add(&outcome.free);
        merged.add_arm(&outcome.merged);
        vote.add_arm(&outcome.vote);
    }

    Ok(FederatedTotals {
        siloed,
        swarmed,
        offfloor,
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
            "{name:<9} {:>7.1}% {:>7} {:>9.1} {:>10.1} {:>9.1} {:>8.1}",
            metrics::ratio(u64::from(self.correct), runs) * 100.0,
            self.decided,
            metrics::ratio(self.turns, runs),
            metrics::ratio(self.crossings, runs),
            metrics::ratio(self.stranded, runs),
            metrics::ratio(self.off_floor_asks, runs),
        )
    }
}

/// Drive a federated scenario through a real agent CLI.
///
/// Every desk gets the same brief, every member gets only its own private
/// facts, and no member can see another desk's transcript. Whether anything
/// crosses a channel is entirely the agents' decision: the harness offers the
/// move in the prompt and writes no mention on anybody's behalf. A run in which
/// nothing crosses is a finding about the agents, not a failure of the harness,
/// and it is reported as such.
fn live_federation(options: &Options, scenario: &Scenario) -> Result<(), String> {
    let channels = scenario.channels();
    if channels.len() < 2 {
        return Err("a federated live run needs at least two [desk ...] sections".to_owned());
    }
    let widest = channels
        .iter()
        .map(|channel| channel.members.len())
        .max()
        .unwrap_or(2);
    let policy = EpisodePolicy {
        turn_budget: turn_budget(widest),
        quorum: QuorumPolicy {
            threshold: quorum_threshold(widest),
            ..options.policy.quorum
        },
        ..options.policy
    };

    let (mut seated, usage_seats) = seat_federation(options, &channels, scenario, policy.quorum)?;
    let seats = seated.len();
    println!(
        "driving {} desks through {}\n\
         {} members, budget {} per desk, quorum {}, referrals {} hops\n\n\
         The brief every desk sees:\n{}",
        channels.len(),
        backend_label(options),
        seats,
        policy.turn_budget,
        policy.quorum.threshold,
        swarm_referrals().max_hops,
        scenario.brief(),
    );
    for channel in &channels {
        println!("{:>10}: {}", channel.name, channel.members.join(", "));
    }
    println!();

    // See the matching comment in `live_round`: a plain loop rather than
    // `.collect()` keeps dropck from extending `seated`'s borrow.
    let mut loose: Vec<&mut dyn SwarmMember> = Vec::new();
    for member in &mut seated {
        loose.push(member.as_mut());
    }
    let mut members = swarm::group_by_desk(&channels, loose.into_iter());
    let wall = Instant::now();
    let report = swarm::drive_swarm(
        &channels,
        &mut members,
        &policy,
        swarm_referrals(),
        // A live federation asks off the floor too, and for the same reason a
        // simulated one does: a desk that spends its authorized turns asking
        // has none left to decide with. `--ask-cap 0` puts it back on the
        // floor, which is what every recorded live run used.
        if options.ask_cap == 0 {
            AskChannel::OnFloor
        } else {
            AskChannel::OffFloor {
                cap: options.ask_cap,
            }
        },
        &scenario.brief(),
        true,
    )?;
    let wall = wall.elapsed();
    for line in &report.trace {
        println!("{line}");
    }

    println!();
    for desk in &report.desks {
        println!(
            "{:>10} ended {} on {}",
            desk.name,
            desk.ending.label(),
            desk.decided
                .as_ref()
                .map_or_else(|| "nothing".to_owned(), |topic| format!("#{topic}")),
        );
    }
    let decided = report
        .decided
        .as_ref()
        .map(tinyhivemind_hive::trace::TopicId::as_str);
    println!(
        "\nhive   federation decided {} after {} agent turns in {:.0} s — {}",
        decided.map_or_else(|| "nothing".to_owned(), |topic| format!("#{topic}")),
        report.turns,
        wall.as_secs_f64(),
        verdict(decided, &scenario.truth),
    );
    println!(
        "       {} messages crossed a channel, {} answers arrived too late, {} turn(s) on !defer",
        report.crossings, report.stranded, report.defers,
    );
    print_usage(options, &usage_seats);

    let backend = poll_backend(options)?;
    let picks = live::poll(options, scenario, &backend)?;
    for (id, pick) in &picks {
        println!("vote   {id:>16} alone: #{pick}");
    }
    match plurality(&picks) {
        Some(topic) => println!(
            "vote   plurality #{topic} of {} — {}",
            picks.len(),
            verdict(Some(topic.as_str()), &scenario.truth),
        ),
        None => println!("vote   tied, no plurality — no answer"),
    }
    Ok(())
}

/// Desks named individually in the banner.
///
/// A federation of two hundred would otherwise spend a screen on it, and the
/// shape of the arrangement is already carried by the count and the
/// distinctness line beneath.
const NAMED_DESKS: usize = 6;

/// Print what the federation is, before any arm runs.
fn describe(
    options: &Options,
    first: &Federation,
    desk_policy: &EpisodePolicy,
    merged_policy: &EpisodePolicy,
    count: usize,
) {
    println!(
        "federations {}  desks {}  per desk {}  options {}  desk bias +{}  eval noise ±{}\n\
         per-desk policy: budget {}  quorum {}   merged: budget {}  quorum {}\n\
         referrals: {} hops, desk mentions and returns on\n",
        count,
        first.desks.len(),
        options.per_desk,
        options.topics,
        options.bias,
        options.noise,
        desk_policy.turn_budget,
        desk_policy.quorum.threshold,
        merged_policy.turn_budget,
        merged_policy.quorum.threshold,
        swarm_referrals().max_hops,
    );
    print!("the federation: ");
    for desk in first.desks.iter().take(NAMED_DESKS) {
        print!("{} overrates #{}  ", desk.name, desk.decoy);
    }
    if first.desks.len() > NAMED_DESKS {
        print!("… and {} more  ", first.desks.len() - NAMED_DESKS);
    }
    println!("and #{} is genuinely best", first.truth);
    if first.decoys_distinct {
        println!();
    } else {
        // Said out loud rather than left to the reader to derive from the desk
        // and option counts: with more desks than non-truth options some desks
        // necessarily share a blind spot, and pooling across two desks that are
        // wrong about the same thing imports the error instead of cancelling
        // it. Any number read off such a run is measuring a different task.
        println!(
            "note: {} desks share {} non-truth options, so some desks are wrong about the same one\n",
            first.desks.len(),
            first.topics.len().saturating_sub(1),
        );
    }
}

/// Print one federated episode, channel by channel.
fn trace_swarm(first: &Federation, desk_policy: &EpisodePolicy) -> Result<(), String> {
    let report = run_swarm(
        first,
        desk_policy,
        swarm_referrals(),
        AskChannel::OnFloor,
        TASK,
        true,
    )?;
    for line in &report.trace {
        println!("{line}");
    }
    println!();
    for desk in &report.desks {
        println!(
            "{:>9} ended {} on {}",
            desk.name,
            desk.ending.label(),
            desk.decided
                .as_ref()
                .map_or_else(|| "nothing".to_owned(), |topic| format!("#{topic}")),
        );
    }
    println!(
        "federation decided {} ({}) after {} agent turns, {} crossings\n",
        report
            .decided
            .as_ref()
            .map_or_else(|| "nothing".to_owned(), |topic| format!("#{topic}")),
        if report.correct { "correct" } else { "wrong" },
        report.turns,
        report.crossings,
    );
    Ok(())
}

/// Print the federated comparison table and the totals under it.
fn tabulate(totals: &FederatedTotals, wall: std::time::Duration, federations: usize) {
    let FederatedTotals {
        siloed,
        swarmed,
        offfloor,
        free,
        merged,
        vote,
    } = totals;
    println!(
        "{:<9} {:>8} {:>8} {:>9} {:>10} {:>9} {:>8}",
        "arm", "correct", "decided", "turns", "crossings", "stranded", "asks/ep",
    );
    println!("{}", siloed.row("siloed"));
    println!("{}", swarmed.row("swarm"));
    println!("{}", offfloor.row("swarm°"));
    println!("{}", free.row("pooled"));
    println!(
        "{:<9} {:>7.1}% {:>7} {:>9.1} {:>10} {:>9} {:>8}",
        "merged",
        merged.accuracy(),
        merged.converged,
        merged.turns_per_episode(),
        "—",
        "—",
        "—",
    );
    println!(
        "{:<9} {:>7.1}% {:>7} {:>9.1} {:>10} {:>9} {:>8}",
        "vote",
        vote.accuracy(),
        vote.converged,
        vote.turns_per_episode(),
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

/// One federation's seats, each knowing which channel it sits on, and the
/// usage bookkeeping for whichever of them ran over the HTTP backend.
type FederationSeats = (Vec<Box<dyn SwarmMember>>, Vec<SeatUsage>);

/// Build one seat per federation member, each knowing which channel it sits
/// on, over whichever backend `--api-base` or `--agent-cmd` selects.
///
/// # Errors
///
/// Returns a message naming an unknown member, or the missing command, key,
/// or base a seat needed.
fn seat_federation(
    options: &Options,
    channels: &[Channel],
    scenario: &Scenario,
    quorum: QuorumPolicy,
) -> Result<FederationSeats, String> {
    let mut seated: Vec<Box<dyn SwarmMember>> = Vec::new();
    let mut usage_seats: Vec<SeatUsage> = Vec::new();
    for channel in channels {
        let peers: Vec<(String, String)> = channels
            .iter()
            .filter(|other| other.id != channel.id)
            .map(|other| (other.id.clone(), other.name.clone()))
            .collect();
        for member in &channel.members {
            let Some(agent) = scenario.agents.iter().find(|agent| &agent.id == member) else {
                return Err(format!("desk {} names unknown member {member}", channel.id));
            };
            let private = Scenario::private_brief(agent);
            if options.api_base.is_some() {
                let config = http_config(options)?;
                let model = seat_model(options, agent).to_owned();
                let prompt = AgentPrompt::new(&agent.id, &agent.role, quorum, private);
                let http_agent = HttpAgent::new(prompt, config, model.clone());
                let handle = http_agent.usage_handle();
                seated.push(Box::new(HttpDeskAgent::new(
                    http_agent,
                    channel.name.clone(),
                    peers.clone(),
                )));
                usage_seats.push(SeatUsage {
                    agent_id: agent.id.clone(),
                    model,
                    tier: agent.tier.clone(),
                    handle,
                });
                continue;
            }
            let command = seat_command(options, agent)?;
            let Some(live) = LiveAgent::new(
                &agent.id,
                &agent.role,
                command,
                quorum,
                private,
                options.timeout,
            ) else {
                return Err(format!("could not build agent from {command:?}"));
            };
            seated.push(Box::new(live::LiveDeskAgent::new(
                live,
                channel.name.clone(),
                peers.clone(),
            )));
        }
    }
    Ok((seated, usage_seats))
}
