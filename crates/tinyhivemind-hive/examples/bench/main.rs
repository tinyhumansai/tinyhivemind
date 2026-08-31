//! Simulate and benchmark bounded group deliberation.
//!
//! ```sh
//! cargo run --release -p tinyhivemind-hive --example bench            # compare arms
//! cargo run --release -p tinyhivemind-hive --example bench -- --trace # one episode
//! cargo run --release -p tinyhivemind-hive --example bench -- --sweep # tune the policy
//! cargo run -p tinyhivemind-hive --example bench -- --agent-cmd "opencode run"
//! ```
//!
//! # What is being measured
//!
//! A room of agents chooses between several options, exactly one of which is
//! genuinely best. Each member holds a private, noisy evaluation of every
//! option, so no member is individually reliable. Three arms decide the same
//! rooms from the same private evaluations:
//!
//! - `ladder` — the responder ladder in `tinyhivemind` picks one responder and
//!   that agent answers alone. One turn. This is today's behaviour.
//! - `vote` — the honest matched-budget control: independent answers, decided
//!   by plurality, with nobody seeing anybody.
//! - `hive` — a deliberation episode at the same budget.
//!
//! Two numbers matter and they are different kinds of number. **correct %** is
//! a claim about the protocol on this synthetic task and nothing more — it is
//! not evidence that real models deliberate better. **ns/step** is a claim
//! about this library: how much a host pays to run the state machine.

mod arms;
mod live;
mod metrics;
mod rng;
mod run;
mod sim;
mod sweep;

use std::time::Instant;

use tinyhivemind_hive::{EpisodePolicy, QuorumPolicy};

use crate::live::LiveAgent;
use crate::metrics::{Aggregate, arm_header, arm_row};
use crate::run::{Participant, drive, run_episode};
use crate::sim::Room;

/// The task every room is given.
const TASK: &str = "We must choose one rollout strategy for a risky migration. Decide together.";

/// Parsed command line.
struct Options {
    episodes: u32,
    agents: usize,
    topics: usize,
    noise: u32,
    seed: u64,
    mode: Mode,
}

/// What this run does.
enum Mode {
    /// Compare the three arms.
    Compare,
    /// Print one episode turn by turn.
    Trace,
    /// Search the policy grid.
    Sweep,
    /// Drive one episode through a real agent CLI.
    Live(String),
}

impl Options {
    /// Read the command line, falling back to defaults.
    fn parse() -> Self {
        let mut options = Self {
            episodes: 500,
            agents: 5,
            topics: 4,
            noise: 45,
            seed: 1,
            mode: Mode::Compare,
        };
        let mut args = std::env::args().skip(1);
        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--episodes" => options.episodes = next_number(&mut args).unwrap_or(500),
                "--agents" => {
                    options.agents = usize::try_from(next_number(&mut args).unwrap_or(5)).unwrap_or(5);
                }
                "--topics" => {
                    options.topics = usize::try_from(next_number(&mut args).unwrap_or(4)).unwrap_or(4);
                }
                "--noise" => options.noise = next_number(&mut args).unwrap_or(45),
                "--seed" => options.seed = u64::from(next_number(&mut args).unwrap_or(1)),
                "--trace" => options.mode = Mode::Trace,
                "--sweep" => options.mode = Mode::Sweep,
                "--agent-cmd" => {
                    if let Some(command) = args.next() {
                        options.mode = Mode::Live(command);
                    }
                }
                _ => {}
            }
        }
        options
    }
}

fn next_number(args: &mut impl Iterator<Item = String>) -> Option<u32> {
    args.next()?.parse().ok()
}

/// The policy the comparison runs at: the crate default, narrowed to the
/// budget the control arms are matched against.
fn comparison_policy() -> EpisodePolicy {
    EpisodePolicy {
        turn_budget: 9,
        quorum: QuorumPolicy {
            threshold: 2,
            window: 100,
            require_grounded: true,
        },
        ..EpisodePolicy::DEFAULT
    }
}

fn main() {
    let options = Options::parse();
    if let Err(error) = run(&options) {
        eprintln!("bench failed: {error}");
        std::process::exit(1);
    }
}

/// Run the selected mode.
fn run(options: &Options) -> Result<(), String> {
    let rooms: Vec<Room> = (0..options.episodes)
        .map(|index| {
            Room::generate(
                options.seed ^ u64::from(index),
                options.agents,
                options.topics,
                options.noise,
            )
        })
        .collect();

    match &options.mode {
        Mode::Compare => compare(options, &rooms),
        Mode::Trace => trace(&rooms),
        Mode::Sweep => sweep_policies(options, &rooms),
        Mode::Live(command) => live_episode(options, command),
    }
}

/// Run the three arms over the same rooms and print the comparison.
fn compare(options: &Options, rooms: &[Room]) -> Result<(), String> {
    let policy = comparison_policy();
    println!(
        "rooms {}  agents {}  options {}  eval noise ±{}  budget {}  quorum {}\n",
        rooms.len(),
        options.agents,
        options.topics,
        options.noise,
        policy.turn_budget,
        policy.quorum.threshold,
    );

    let mut hive = Aggregate::default();
    let mut vote = Aggregate::default();
    let mut ladder = Aggregate::default();
    let wall = Instant::now();
    for (index, room) in rooms.iter().enumerate() {
        hive.add(&run_episode(room, &policy, TASK, false)?);
        let seed = options.seed ^ u64::try_from(index).unwrap_or(0);
        ladder.add_arm(&arms::run_ladder(room, seed)?);
        vote.add_arm(&arms::run_vote(room, policy.turn_budget));
    }
    let wall = wall.elapsed();

    println!("{}", arm_header());
    println!("{}", arm_row("ladder", &ladder));
    println!("{}", arm_row("vote", &vote));
    println!("{}", arm_row("hive", &hive));

    println!(
        "\nhive endings: converged {} · deadlocked {} · exhausted {} · idle {}",
        hive.converged, hive.deadlocked, hive.exhausted, hive.idle,
    );
    println!(
        "library time {:.1} ms over {} steps  ({:.0} ns/step, {:.0} episodes/s)",
        hive.library_time.as_secs_f64() * 1_000.0,
        hive.step_calls,
        hive.nanos_per_step(),
        hive.episodes_per_second(),
    );
    println!(
        "wall clock {:.1} ms for {} episodes across all three arms",
        wall.as_secs_f64() * 1_000.0,
        rooms.len(),
    );
    Ok(())
}

/// Print one episode turn by turn.
fn trace(rooms: &[Room]) -> Result<(), String> {
    let Some(room) = rooms.first() else {
        return Err("no rooms generated".to_owned());
    };
    let policy = comparison_policy();
    let report = run_episode(room, &policy, TASK, true)?;
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

/// Search the policy grid and print the ordering.
fn sweep_policies(options: &Options, rooms: &[Room]) -> Result<(), String> {
    println!(
        "sweeping {} policies over {} rooms\n",
        192,
        rooms.len()
    );
    let wall = Instant::now();
    let scored = sweep::sweep(rooms, TASK)?;
    let wall = wall.elapsed();

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
            "\nbest policy: {:.1}% correct at {:.2} turns/episode, against {:.1}% for the \
             matched-budget vote",
            best.totals.accuracy(),
            best.totals.turns_per_episode(),
            vote.accuracy(),
        );
    }
    println!(
        "swept in {:.2} s ({} episodes, {} agents each)",
        wall.as_secs_f64(),
        scored.len() * rooms.len(),
        options.agents,
    );
    Ok(())
}

/// Drive one episode through a real agent CLI.
fn live_episode(options: &Options, command: &str) -> Result<(), String> {
    let room = Room::generate(options.seed, options.agents, options.topics, options.noise);
    let ids = room.member_ids();
    let roles = [
        "planner, who proposes concrete options",
        "critic, who looks for the weakness in a proposal",
        "archivist, who supplies precedent and evidence",
        "scout, who looks for the option nobody has raised",
        "auditor, who checks the decision against the constraints",
    ];
    let mut agents: Vec<LiveAgent> = ids
        .iter()
        .enumerate()
        .filter_map(|(index, id)| {
            LiveAgent::new(id, roles.get(index).copied().unwrap_or("teammate"), command)
        })
        .collect();
    if agents.len() != ids.len() {
        return Err(format!("could not build agents from {command:?}"));
    }
    let mut participants: Vec<&mut dyn Participant> = agents
        .iter_mut()
        .map(|agent| agent as &mut dyn Participant)
        .collect();

    let policy = comparison_policy();
    println!("driving one episode through {command:?}\n");
    let report = drive(&ids, &mut participants, &policy, TASK, true)?;
    for line in &report.trace {
        println!("{line}");
    }
    println!(
        "\nended {} on {} after {} turns; {:.0} ns/step of library time",
        report.ending.label(),
        report
            .decided
            .as_ref()
            .map_or_else(|| "nothing".to_owned(), |topic| format!("#{topic}")),
        report.turns,
        metrics::ratio(
            u64::try_from(report.library_time.as_nanos()).unwrap_or(u64::MAX),
            u64::from(report.step_calls),
        ),
    );
    Ok(())
}
