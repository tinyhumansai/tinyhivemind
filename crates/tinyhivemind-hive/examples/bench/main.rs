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
//! - `hive+` — the same, at the policy `--sweep` picks.
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
//! | `--agents N` | members per room (default 5) |
//! | `--topics N` | options on offer (default 4) |
//! | `--noise N` | half-width of the error on a private evaluation (default 90) |
//! | `--seed N` | room generator seed (default 1) |
//! | `--budget N`, `--quorum N`, `--window N` | episode policy |
//! | `--dominance N`, `--repetition N`, `--no-blind` | episode policy |
//! | `--trace` | print one episode turn by turn |
//! | `--sweep` | score the policy grid |
//! | `--agent-cmd CMD` | drive one episode through a real agent CLI |

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
    /// Rooms to simulate.
    episodes: u32,
    /// Members per room.
    agents: usize,
    /// Options on offer.
    topics: usize,
    /// Half-width of the error on each private evaluation.
    noise: u32,
    /// Room generator seed.
    seed: u64,
    /// The policy every mode but the sweep runs at.
    policy: EpisodePolicy,
    /// What this run does.
    mode: Mode,
}

/// What this run does.
enum Mode {
    /// Compare every arm.
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
            noise: 90,
            seed: 1,
            policy: tuned_policy(),
            mode: Mode::Compare,
        };
        let mut args = std::env::args().skip(1);
        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--episodes" => options.episodes = next_number(&mut args).unwrap_or(500),
                "--agents" => {
                    options.agents =
                        usize::try_from(next_number(&mut args).unwrap_or(5)).unwrap_or(5);
                }
                "--topics" => {
                    options.topics =
                        usize::try_from(next_number(&mut args).unwrap_or(4)).unwrap_or(4);
                }
                "--noise" => options.noise = next_number(&mut args).unwrap_or(90),
                "--seed" => options.seed = u64::from(next_number(&mut args).unwrap_or(1)),
                "--budget" => {
                    options.policy.turn_budget = next_number(&mut args).unwrap_or(12);
                }
                "--quorum" => {
                    options.policy.quorum.threshold = next_number(&mut args).unwrap_or(3);
                }
                "--window" => {
                    options.policy.quorum.window = next_number(&mut args).unwrap_or(100);
                }
                "--dominance" => {
                    options.policy.dominance_cap = next_number(&mut args).unwrap_or(40);
                }
                "--repetition" => {
                    options.policy.repetition_cap = next_number(&mut args).unwrap_or(2);
                }
                "--no-blind" => options.policy.blind_round = false,
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

/// Read the next argument as a number.
fn next_number(args: &mut impl Iterator<Item = String>) -> Option<u32> {
    args.next()?.parse().ok()
}

/// The crate's own conservative default, with the window widened to cover a
/// whole episode so the two hive arms differ only in the knobs the sweep moved.
fn default_policy() -> EpisodePolicy {
    EpisodePolicy {
        quorum: QuorumPolicy {
            window: 100,
            ..QuorumPolicy::DEFAULT
        },
        ..EpisodePolicy::DEFAULT
    }
}

/// The policy `--sweep` picks, and the one every other mode runs at.
///
/// The load-bearing change is the quorum threshold. Five members can put two
/// grounded supporters behind each of two options, and an episode in which two
/// options both carry is deadlocked by definition: no amount of further
/// support resolves it, because both stay above the line. A threshold above
/// half the desk makes that state unreachable and the deadlock rate falls to
/// zero. The wider budget pays for the extra supporter each decision now needs.
fn tuned_policy() -> EpisodePolicy {
    EpisodePolicy {
        turn_budget: 12,
        blind_round: true,
        dominance_cap: 40,
        repetition_cap: 2,
        quorum: QuorumPolicy {
            threshold: 3,
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
        Mode::Trace => trace(&rooms, &options.policy),
        Mode::Sweep => sweep_policies(options, &rooms),
        Mode::Live(command) => live_episode(options, command),
    }
}

/// Run every arm over the same rooms and print the comparison.
fn compare(options: &Options, rooms: &[Room]) -> Result<(), String> {
    let tuned = options.policy;
    let default = default_policy();
    println!(
        "rooms {}  agents {}  options {}  eval noise ±{}\n\
         tuned policy: budget {}  quorum {}  blind {}  dominance {}  repetition {}\n",
        rooms.len(),
        options.agents,
        options.topics,
        options.noise,
        tuned.turn_budget,
        tuned.quorum.threshold,
        if tuned.blind_round { "yes" } else { "no" },
        tuned.dominance_cap,
        tuned.repetition_cap,
    );

    let mut hive_default = Aggregate::default();
    let mut hive_tuned = Aggregate::default();
    let mut vote = Aggregate::default();
    let mut ladder = Aggregate::default();
    let wall = Instant::now();
    for (index, room) in rooms.iter().enumerate() {
        hive_default.add(&run_episode(room, &default, TASK, false)?);
        hive_tuned.add(&run_episode(room, &tuned, TASK, false)?);
        let seed = options.seed ^ u64::try_from(index).unwrap_or(0);
        ladder.add_arm(&arms::run_ladder(room, seed)?);
        // The control is given the whole budget, which is more turns than the
        // deliberation actually spends. It is the arm to beat, so it gets
        // every advantage.
        vote.add_arm(&arms::run_vote(room, tuned.turn_budget));
    }
    let wall = wall.elapsed();

    println!("{}", arm_header());
    println!("{}", arm_row("ladder", &ladder));
    println!("{}", arm_row("vote", &vote));
    println!("{}", arm_row("hive", &hive_default));
    println!("{}", arm_row("hive+", &hive_tuned));

    println!(
        "\nhive  endings: converged {} · deadlocked {} · exhausted {} · idle {}",
        hive_default.converged, hive_default.deadlocked, hive_default.exhausted, hive_default.idle,
    );
    println!(
        "hive+ endings: converged {} · deadlocked {} · exhausted {} · idle {}",
        hive_tuned.converged, hive_tuned.deadlocked, hive_tuned.exhausted, hive_tuned.idle,
    );
    println!(
        "library time {:.1} ms over {} steps ({:.0} ns/step, {:.0} episodes/s)",
        hive_tuned.library_time.as_secs_f64() * 1_000.0,
        hive_tuned.step_calls,
        hive_tuned.nanos_per_step(),
        hive_tuned.episodes_per_second(),
    );
    println!(
        "wall clock {:.1} ms for {} rooms across every arm",
        wall.as_secs_f64() * 1_000.0,
        rooms.len(),
    );
    Ok(())
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

/// Search the policy grid and print the ordering.
fn sweep_policies(options: &Options, rooms: &[Room]) -> Result<(), String> {
    let wall = Instant::now();
    let scored = sweep::sweep(rooms, TASK)?;
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

/// Drive one episode through a real agent CLI.
fn live_episode(options: &Options, command: &str) -> Result<(), String> {
    let room = Room::generate(options.seed, options.agents, options.topics, options.noise);
    let ids = room.member_ids();
    let roles = [
        "planner, who proposes concrete options",
        "critic, who looks for the weakness in a proposal",
        "archivist, who supplies precedent and evidence",
        "scout, who looks for the option nobody has raised",
        "auditor, who checks a decision against the constraints",
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

    println!("driving one episode through {command:?}\n");
    let report = drive(&ids, &mut participants, &options.policy, TASK, true)?;
    for line in &report.trace {
        println!("{line}");
    }
    println!(
        "\nended {} on {} after {} turns, {:.0} ns/step of library time",
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
