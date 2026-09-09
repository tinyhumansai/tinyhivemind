//! Simulate and benchmark bounded group deliberation.
//!
//! ```sh
//! cargo run --release -p tinyhivemind-hive --example bench            # compare arms
//! cargo run --release -p tinyhivemind-hive --example bench -- --trace # one episode
//! cargo run --release -p tinyhivemind-hive --example bench -- --sweep # tune the policy
//! cargo run --release -p tinyhivemind-hive --example bench -- --swarm # several desks
//! cargo run -p tinyhivemind-hive --example bench -- --agent-cmd "opencode run"
//! ```
//!
//! # What is being measured
//!
//! A room of agents chooses between several options, exactly one of which is
//! genuinely best. Every member holds a private, noisy evaluation of every
//! option, so no member is individually reliable and the room's only route to
//! the right answer is to pool what its members separately believe. Four arms
//! decide the same rooms from the same private evaluations:
//!
//! - `ladder` — the responder ladder in `tinyhivemind` selects one responder and
//!   that agent answers alone. One turn. This is today's behaviour.
//! - `vote` — the honest matched-budget control: independent answers decided by
//!   plurality, with nobody seeing anybody. It is given the whole budget, which
//!   is more turns than the deliberation actually spends.
//! - `hive` — a deliberation episode at [`EpisodePolicy::DEFAULT`].
//! - `hive+` — the same, at the tuned policy: a majority quorum that is never
//!   unanimity, and three turns of budget per member, both scaled to the desk.
//! - `hive+dir`, `hive+defer`, `hive+dir+defer` — the tuned policy with the
//!   folded transactive-memory directory on, with `!defer` bounded, and with
//!   both. See the README's "Delegation" section.
//! - `ladder+dir` — the responder ladder again, this time given a directory
//!   the room earned over `--history` prior episodes of `hive+`.
//!
//! # What the numbers mean
//!
//! **correct %** is a claim about this protocol on this synthetic task and
//! nothing more. It is not evidence that language models deliberate better,
//! and it cannot be: the participants here are arithmetic. What it does show
//! is which *policy* aggregates information and which throws it away, which is
//! the question a host actually has to answer when it configures a room.
//!
//! **ns/step** is a claim about this library — what a host pays to run the
//! state machine once, with the agents' own time excluded.
//!
//! # Command line
//!
//! | flag | meaning |
//! | --- | --- |
//! | `--episodes N` | rooms to simulate (default 500) |
//! | `--topics N` | options on offer (default 4) |
//! | `--noise N` | half-width of the error on a private evaluation (default 90) |
//! | `--agents N` | members per room; moves the tuned quorum and budget with it |
//! | `--seed N` | room generator seed (default 1) |
//! | `--budget N`, `--quorum N`, `--window N` | episode policy |
//! | `--dominance N`, `--repetition N`, `--no-blind` | episode policy |
//! | `--trace` | print one episode turn by turn |
//! | `--sweep` | score the policy grid |
//! | `--swarm` | several desks messaging across channels |
//! | `--desks N`, `--per-desk N`, `--bias N` | the federation `--swarm` builds |
//! | `--agent-cmd CMD` | drive one episode through a real agent CLI |
//! | `--scenario PATH` | give the live room a real problem with private facts |
//! | `--repeat N` | run a live scenario N times and count both arms |
//! | `--timeout SECS` | per-turn deadline for a live agent or HTTP request (default 180) |
//! | `--api-base URL` | drive seats directly over HTTP instead of a CLI |
//! | `--api-key-env NAME` | env var carrying the HTTP backend's key (default `LADDER_API_KEY`) |
//! | `--model NAME` | the HTTP backend's default model (default `flash`) |
//! | `--wire openai\|anthropic` | which chat wire format the HTTP backend speaks (default `openai`) |
//! | `--model-cost model=N` | cost per 1000 tokens for `model`, for the usage table (repeatable) |
//! | `--seat-model agent_id=model`, `--seat-cmd agent_id="command"` | per-seat backend override |
//! | `--specialist-model NAME` | model for a seat the scenario marks as a specialist |
//! | `--specialists N`, `--hidden-profile` | how expertise is distributed |
//! | `--defer-cap N`, `--history N`, `--cost-tiers` | the delegation arms |
//! | `--aside-cap N` | pairwise checks one member may open (default 1); `0` makes every on-floor and alongside aside arm identical to `hive+` |
//! | `--exchange-cap N` | private rows one member may write off the floor (default 4); `0` disables `hive+rounds` |
//! | `--blind-evidence` | members open the blind round with a deposit, not a position |
//! | `--directory` | fold the directory into the traced episode's own policy |
//! | `--thinking on\|off` | whether the HTTP backend reasons before answering |
//!
//! See `live.rs` and `http.rs` for what the two live backends drive.

mod arms;
mod backend;
mod budget;
mod cli;
mod compare;
mod context;
mod federation;
mod horizon;
mod http;
mod live;
mod live_single;
mod live_swarm;
mod metrics;
mod policy;
mod rng;
mod run;
mod scale;
mod scenario;
mod sim;
mod swarm;
mod sweep;
mod variety;

use std::time::Instant;

use tinyhivemind_hive::EpisodePolicy;

use crate::cli::{Mode, Options};
use crate::compare::compare;
use crate::live_single::live_episode;
use crate::live_swarm::swarm_compare;
use crate::metrics::{Aggregate, paired_bootstrap, spearman_milli, wilson};
use crate::rng::mix;
use crate::run::run_episode;
use crate::sim::Room;

/// The task every room is given.
pub(crate) const TASK: &str =
    "We must choose one rollout strategy for a risky migration. Decide together.";

fn main() {
    let options = Options::parse();
    if matches!(options.mode, Mode::StatsCheck) {
        if stats_check() {
            println!("stats-check: ok");
        } else {
            eprintln!("stats-check: FAILED");
            std::process::exit(1);
        }
        return;
    }
    if let Err(error) = run(&options) {
        eprintln!("bench failed: {error}");
        std::process::exit(1);
    }
}

/// Run known cases through `wilson`, `paired_bootstrap` and `spearman_milli`
/// and report whether every one came out as it must.
///
/// `metrics.rs` is an example file, so `cargo test` never runs a `#[test]`
/// placed in it; this is that coverage's stand-in, wired to CI through the
/// join the README's `--stats-check` line names. Each case is a property the
/// statistic is defined to have, not a fitted expectation, so a break here is
/// always a real regression rather than a brittle golden number.
fn stats_check() -> bool {
    let mut ok = true;

    // wilson(0, n): the observed rate is 0%, so the interval cannot go
    // negative, and with ten trials of silence it has not pinned the rate to
    // exactly 0% either.
    let (zero_low, zero_high) = wilson(0, 10);
    ok &= zero_low == 0.0 && zero_high > 0.0 && zero_high < 100.0;

    // wilson(n, n): the mirror image, pinned at the top rather than the
    // bottom.
    let (full_low, full_high) = wilson(10, 10);
    ok &= (full_high - 100.0).abs() < 1e-9 && full_low > 0.0 && full_low < 100.0;

    // Identical rankings correlate perfectly; the reverse of one anti-correlates
    // perfectly. Both are exact integers by construction, not approximations.
    let increasing = [1_u32, 2, 3, 4, 5, 6, 7];
    let decreasing = [7_u32, 6, 5, 4, 3, 2, 1];
    ok &= spearman_milli(&increasing, &increasing) == 1000;
    ok &= spearman_milli(&increasing, &decreasing) == -1000;

    // The same two properties with tied ranks, which is the case the
    // no-ties shortcut formula gets wrong: it biases the magnitude toward
    // zero, where Pearson-on-ranks still reports the perfect correlation
    // that is actually there.
    let tied = [1_u32, 1, 2, 2, 3, 3];
    let tied_reversed = [3_u32, 3, 2, 2, 1, 1];
    ok &= spearman_milli(&tied, &tied) == 1000;
    ok &= spearman_milli(&tied, &tied_reversed) == -1000;

    // A vector with no spread has no ranking to correlate with anything.
    let flat = [4_u32, 4, 4, 4, 4, 4];
    ok &= spearman_milli(&flat, &tied) == 0;

    // And the magnitude stays inside the definition's bounds on a partial
    // correlation, ties and all.
    let partial = spearman_milli(&[1_u32, 1, 2, 3, 3, 4], &[1_u32, 2, 2, 3, 4, 4]);
    ok &= partial > 0 && partial < 1000;

    // A paired bootstrap of an array against itself can never show a
    // difference, at any resample, so both bounds collapse to zero.
    let flags = [true, false, true, true, false, false, true, false];
    let (diff_low, diff_high) = paired_bootstrap(&flags, &flags, 7, 256);
    ok &= diff_low == 0.0 && diff_high == 0.0;

    // The check arms have properties of their own, and `sim.rs` is an example
    // file too. Folded in here rather than behind a second flag so CI keeps
    // running one command.
    ok &= crate::sim::check_selfcheck();

    ok
}

/// Run the selected mode.
fn run(options: &Options) -> Result<(), String> {
    // Built first because every other mode needs them, and skipped for the
    // swarm, which generates federations of its own.
    if matches!(options.mode, Mode::ScaleSweep) {
        // Its own rooms, one set per size, so nothing here generates a room
        // at the single `--agents` size that would then go unused.
        return scale::sweep(options);
    }
    if matches!(options.mode, Mode::StageSweep) {
        // Its own rooms, one chain of them per horizon, so nothing here
        // generates a single-stage room that would then go unused.
        return horizon::sweep(options);
    }
    if matches!(options.mode, Mode::FacetSweep) {
        // Its own rooms, one per facet of each task, so nothing here
        // generates a single-facet room that would then go unused.
        return variety::sweep(options);
    }
    if matches!(options.mode, Mode::Swarm) {
        return swarm_compare(options);
    }
    let rooms: Vec<Room> = (0..options.episodes)
        .map(|index| {
            // Mixed rather than xor-ed: `seed ^ index` over a range of
            // indices produces almost the same *set* of room seeds for two
            // neighbouring seeds, which would make one seed's run silently
            // indistinguishable from the next.
            Room::generate_with(
                mix(options.seed, u64::from(index)),
                options.agents,
                options.topics,
                options.noise,
                options.expertise,
                options.cost,
            )
        })
        .map(|mut room| {
            // Applied to the generated room rather than threaded through
            // `generate_with`: it changes nothing about the private
            // evaluations, only what the members do with them, so a run
            // without the flag draws exactly the room it always drew.
            room.set_blind_evidence(options.blind_evidence);
            room
        })
        .collect();

    match &options.mode {
        // Handled above, before the single-desk rooms were generated.
        // Handled above, each before the rooms this arm of the match would
        // have needed were generated.
        Mode::Swarm | Mode::StatsCheck | Mode::ScaleSweep | Mode::StageSweep => Ok(()),
        Mode::Compare => compare(options, &rooms),
        Mode::Trace => trace(&rooms, &options.policy),
        Mode::Sweep => sweep_policies(options, &rooms),
        Mode::ContextSweep => sweep_context(options, &rooms),
        Mode::Live => live_episode(options),
    }
}

/// Print one episode turn by turn.
fn trace(rooms: &[Room], policy: &EpisodePolicy) -> Result<(), String> {
    let Some(room) = rooms.first() else {
        return Err("no rooms generated".to_owned());
    };
    let report = run_episode(room, policy, TASK, true)?;
    println!("best option: #{}\n", room.truth);
    for line in &report.trace {
        println!("{line}");
    }
    println!(
        "\nended {} on {} after {} turns ({} correct)",
        report.ending.label(),
        report
            .decided
            .as_ref()
            .map_or_else(|| "nothing".to_owned(), |topic| format!("#{topic}")),
        report.turns,
        if report.correct { "and" } else { "but not" },
    );
    Ok(())
}

/// Charge every arm for the context it needs, and print who degrades first.
fn sweep_context(options: &Options, rooms: &[Room]) -> Result<(), String> {
    let wall = Instant::now();
    let points = budget::sweep(rooms, &options.policy, TASK, options.aside_cap)?;
    let wall = wall.elapsed();
    print!("{}", budget::render(&points, rooms.len()));
    println!("\nswept in {:.2} s", wall.as_secs_f64());
    Ok(())
}

/// Search the policy grid and print the ordering.
fn sweep_policies(options: &Options, rooms: &[Room]) -> Result<(), String> {
    let wall = Instant::now();
    let scored = sweep::sweep(rooms, TASK, options.agents)?;
    let wall = wall.elapsed();
    println!(
        "swept {} policies over {} rooms in {:.2} s\n",
        scored.len(),
        rooms.len(),
        wall.as_secs_f64(),
    );

    println!("{}", sweep::header());
    for point in scored.iter().take(12) {
        println!("{}", sweep::row(point));
    }
    if let Some(worst) = scored.last() {
        println!("...\n{}", sweep::row(worst));
    }

    if let Some(best) = scored.first() {
        let mut vote = Aggregate::default();
        for room in rooms {
            vote.add_arm(&arms::run_vote(room, best.policy.turn_budget));
        }
        println!(
            "\nbest policy: {:.1}% correct at {:.2} turns per episode, against {:.1}% for the \
             matched-budget vote",
            best.totals.accuracy(),
            best.totals.turns_per_episode(),
            vote.accuracy(),
        );
    }
    println!(
        "{} episodes simulated, {} agents each",
        scored.len() * rooms.len(),
        options.agents,
    );
    Ok(())
}
