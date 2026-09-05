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

mod arms;
mod federation;
mod live;
mod metrics;
mod rng;
mod run;
mod scenario;
mod sim;
mod swarm;
mod sweep;

use std::time::Instant;

use tinyhivemind_hive::{EpisodePolicy, QuorumPolicy, trace::TopicId};

use crate::federation::Federation;
use crate::live::LiveAgent;
use crate::metrics::{Aggregate, arm_header, arm_row, paired_bootstrap, spearman_milli, wilson};
use crate::rng::mix;
use crate::run::{Participant, drive, run_episode};
use crate::scenario::Scenario;
use crate::sim::Room;
use crate::swarm::{SwarmReport, pooled, run_swarm};
use tinyhivemind_hive::referral::ReferralPolicy;

/// How much a desk overrates its own decoy, by default.
///
/// The value is bounded on both sides, and both bounds are what make the
/// problem federated rather than merely noisy.
///
/// *Above* the 60-point gap between the true option and a decoy, a desk's own
/// average points at the wrong answer, so no amount of deliberation inside one
/// channel finds the right one. *Below* twice that gap, one outside reading is
/// enough to overturn it: a member that has heard one other desk's reading of
/// its favourite averages `(40 + bias + 40) / 2`, which has to fall under the
/// true option's 100. At three desks the honest window is roughly 90 to 120,
/// and 110 sits inside it with room on both sides.
const SWARM_BIAS: i32 = 110;
/// The same value where the argument parser needs it unsigned.
const SWARM_BIAS_U32: u32 = 110;
const _: () = assert!(SWARM_BIAS as u32 == SWARM_BIAS_U32);
/// Half-width of the individual error on a federated evaluation, by default.
///
/// Small enough that a desk's shared bias survives it — otherwise every desk
/// is individually unbiased and there is nothing for a channel crossing to
/// fix — and large enough that no single member is an oracle for its desk.
const SWARM_NOISE: u32 = 50;

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
    /// A real problem for the live room, if one was given.
    scenario: Option<String>,
    /// How many times to run a live scenario.
    repeat: u32,
    /// Desks in a federation.
    desks: usize,
    /// Members on each desk of a federation.
    per_desk: usize,
    /// How much a desk overrates its own decoy.
    bias: i32,
    /// Whether to print a transcript as well as the totals.
    trace: bool,
    /// The agent command, when one was given.
    agent: Option<String>,
    /// Print one flat JSON object per arm, ahead of the tables.
    json: bool,
    /// Run the hidden self-check over `wilson`, `paired_bootstrap` and
    /// `spearman_milli` and exit, rather than running a mode.
    stats_check: bool,
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
    /// Compare several desks solving one problem across channels.
    Swarm,
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
            policy: tuned_policy(5),
            mode: Mode::Compare,
            scenario: None,
            repeat: 1,
            desks: 3,
            per_desk: 4,
            bias: SWARM_BIAS,
            trace: false,
            agent: None,
            json: false,
            stats_check: false,
        };
        // The policy is rebuilt once the room size is known, then any explicit
        // policy flag is applied over it, so `--agents` moves the quorum
        // threshold with the desk while `--quorum` still overrides it.
        let args: Vec<String> = std::env::args().skip(1).collect();
        // The federation has its own noise default. A desk is only a
        // correlation boundary if its shared bias is legible *through* each
        // member's individual error: at the single-room default of ±90 the
        // bias is swamped, every desk is individually unbiased, and crossing a
        // channel would be measuring nothing. An explicit `--noise` still
        // wins, so the swamped regime can be asked for on purpose.
        if args.iter().any(|argument| argument == "--swarm")
            && flag_number(&args, "--noise").is_none()
        {
            options.noise = SWARM_NOISE;
        }
        if let Some(agents) = flag_number(&args, "--agents") {
            // Clamped to what `Room::generate` will actually build, so the
            // quorum threshold cannot be set for a desk that does not exist.
            options.agents = usize::try_from(agents).unwrap_or(5).clamp(2, 8);
            options.policy = tuned_policy(options.agents);
        }
        let mut args = args.into_iter();
        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--episodes" => options.episodes = next_number(&mut args).unwrap_or(500),
                "--agents" => {
                    let _ = next_number(&mut args);
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
                "--desks" => {
                    options.desks =
                        usize::try_from(next_number(&mut args).unwrap_or(3)).unwrap_or(3);
                }
                "--per-desk" => {
                    options.per_desk =
                        usize::try_from(next_number(&mut args).unwrap_or(4)).unwrap_or(4);
                }
                "--bias" => {
                    options.bias = i32::try_from(next_number(&mut args).unwrap_or(SWARM_BIAS_U32))
                        .unwrap_or(SWARM_BIAS);
                }
                "--swarm" => options.mode = Mode::Swarm,
                "--trace" => {
                    options.trace = true;
                    // `--swarm --trace` prints a federation transcript rather
                    // than a single room's, so the swarm mode keeps the floor.
                    if !matches!(options.mode, Mode::Swarm) {
                        options.mode = Mode::Trace;
                    }
                }
                "--sweep" => options.mode = Mode::Sweep,
                "--agent-cmd" => {
                    if let Some(command) = args.next() {
                        options.agent = Some(command.clone());
                        // `--swarm --agent-cmd` drives a federation rather than
                        // one room, so the swarm mode keeps the floor.
                        if !matches!(options.mode, Mode::Swarm) {
                            options.mode = Mode::Live(command);
                        }
                    }
                }
                "--scenario" => options.scenario = args.next(),
                "--repeat" => options.repeat = next_number(&mut args).unwrap_or(1).max(1),
                "--json" => options.json = true,
                "--stats-check" => options.stats_check = true,
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

/// Read one flag's number out of the whole argument list.
fn flag_number(args: &[String], flag: &str) -> Option<u32> {
    let at = args.iter().position(|argument| argument == flag)?;
    args.get(at + 1)?.parse().ok()
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

/// The policy `--sweep` picks, scaled to the size of the desk.
///
/// The load-bearing knob is the quorum threshold, and it has two bounds rather
/// than one.
///
/// A threshold *above half the desk* is what removes deadlock. Five members can
/// put two grounded supporters behind each of two options, and an episode in
/// which two options both carry is deadlocked by definition: no amount of
/// further support resolves it, because both stay above the line. Requiring a
/// majority makes that state unreachable, and the deadlock rate falls to zero.
///
/// A threshold *below the whole desk* is what keeps a decision reachable.
/// Cross-inhibition removes a silenced advocate from a topic's supporter set
/// and does not put them back, so at unanimity a single grounded `!object`
/// makes quorum unreachable for the rest of the episode. A live three-member
/// room ran into exactly that and spent its whole budget without deciding.
///
/// Between the two: the smallest majority of the desk, and never the whole of
/// it.
fn tuned_policy(agents: usize) -> EpisodePolicy {
    EpisodePolicy {
        turn_budget: turn_budget(agents),
        blind_round: true,
        dominance_cap: 40,
        repetition_cap: 2,
        quorum: QuorumPolicy {
            threshold: quorum_threshold(agents),
            window: 100,
            require_grounded: true,
            // Refutation is off in both control arms, which is the crate
            // default. No topic is ever capped *and* the simulated members
            // never spend a turn on the move, so `hive+ref` differs from
            // `hive+` in exactly one thing rather than in two.
            refutation_cap: None,
            ..QuorumPolicy::DEFAULT
        },
        ..EpisodePolicy::DEFAULT
    }
}

/// The tuned policy with the negative evidence-to-topic link switched on.
///
/// A cap of two is the crate default: one member's assertion should not kill a
/// hypothesis, and two distinct grounded refuters should. This is the arm the
/// mechanism has to earn its place against, and it can lose.
fn refuting_policy(tuned: &EpisodePolicy) -> EpisodePolicy {
    EpisodePolicy {
        quorum: QuorumPolicy {
            refutation_cap: Some(2),
            ..tuned.quorum
        },
        ..*tuned
    }
}

/// The refuting policy with grounds weighed by evidential depth as well.
fn evidential_policy(tuned: &EpisodePolicy) -> EpisodePolicy {
    let refuting = refuting_policy(tuned);
    EpisodePolicy {
        quorum: QuorumPolicy {
            require_evidential: true,
            ..refuting.quorum
        },
        ..refuting
    }
}

/// Turns a desk needs to reach a majority quorum.
///
/// A blind opening round costs one turn per member before anybody has seen
/// anybody, a majority then has to assemble on one option, and the decision
/// has to be recorded. Three turns per member covers that with room for the
/// objections and questions a real room spends turns on, and it is a cap
/// rather than a cost: a five-member room finishes in under seven turns of the
/// fifteen it is allowed. A budget that does not scale with the desk is what
/// makes a larger room look worse than a smaller one — at a fixed twelve, an
/// eight-member room fails to decide a third of the time and scores 64%; at
/// twenty-four it decides 96% of the time and scores 88%.
fn turn_budget(agents: usize) -> u32 {
    u32::try_from(agents).unwrap_or(5).saturating_mul(3).max(6)
}

/// The smallest majority of a desk that still leaves one member to spare.
fn quorum_threshold(agents: usize) -> u32 {
    let agents = u32::try_from(agents).unwrap_or(u32::MAX);
    let majority = agents / 2 + 1;
    majority.min(agents.saturating_sub(1)).max(2)
}

fn main() {
    let options = Options::parse();
    if options.stats_check {
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
    ok &= full_high == 100.0 && full_low > 0.0 && full_low < 100.0;

    // Identical rankings correlate perfectly; the reverse of one anti-correlates
    // perfectly. Both are exact integers by construction, not approximations.
    let increasing = [1_u32, 2, 3, 4, 5, 6, 7];
    let decreasing = [7_u32, 6, 5, 4, 3, 2, 1];
    ok &= spearman_milli(&increasing, &increasing) == 1000;
    ok &= spearman_milli(&increasing, &decreasing) == -1000;

    // A paired bootstrap of an array against itself can never show a
    // difference, at any resample, so both bounds collapse to zero.
    let flags = [true, false, true, true, false, false, true, false];
    let (diff_low, diff_high) = paired_bootstrap(&flags, &flags, 7, 256);
    ok &= diff_low == 0.0 && diff_high == 0.0;

    ok
}

/// Run the selected mode.
fn run(options: &Options) -> Result<(), String> {
    // Built first because every other mode needs them, and skipped for the
    // swarm, which generates federations of its own.
    if matches!(options.mode, Mode::Swarm) {
        return swarm_compare(options);
    }
    let rooms: Vec<Room> = (0..options.episodes)
        .map(|index| {
            // Mixed rather than xor-ed: `seed ^ index` over a range of
            // indices produces almost the same *set* of room seeds for two
            // neighbouring seeds, which would make one seed's run silently
            // indistinguishable from the next.
            Room::generate(
                mix(options.seed, u64::from(index)),
                options.agents,
                options.topics,
                options.noise,
            )
        })
        .collect();

    match &options.mode {
        // Handled above, before the single-desk rooms were generated.
        Mode::Swarm => Ok(()),
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

    let refuting = refuting_policy(&tuned);
    let evidential = evidential_policy(&tuned);
    let mut hive_default = Aggregate::default();
    let mut hive_tuned = Aggregate::default();
    let mut hive_refuting = Aggregate::default();
    let mut hive_evidential = Aggregate::default();
    let mut vote = Aggregate::default();
    let mut ladder = Aggregate::default();
    let wall = Instant::now();
    for (index, room) in rooms.iter().enumerate() {
        hive_default.add(&run_episode(room, &default, TASK, false)?);
        hive_tuned.add(&run_episode(room, &tuned, TASK, false)?);
        hive_refuting.add(&run_episode(room, &refuting, TASK, false)?);
        hive_evidential.add(&run_episode(room, &evidential, TASK, false)?);
        let seed = mix(options.seed, u64::try_from(index).unwrap_or(0));
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
    println!("{}", arm_row("hive+ref", &hive_refuting));
    println!("{}", arm_row("hive+ev", &hive_evidential));

    println!(
        "\nhive  endings: converged {} · deadlocked {} · exhausted {} · idle {}",
        hive_default.converged, hive_default.deadlocked, hive_default.exhausted, hive_default.idle,
    );
    println!(
        "hive+ endings: converged {} · deadlocked {} · exhausted {} · idle {}",
        hive_tuned.converged, hive_tuned.deadlocked, hive_tuned.exhausted, hive_tuned.idle,
    );
    println!(
        "hive+ref endings: converged {} · deadlocked {} · exhausted {} · idle {}",
        hive_refuting.converged,
        hive_refuting.deadlocked,
        hive_refuting.exhausted,
        hive_refuting.idle,
    );
    println!(
        "hive+ev endings: converged {} · deadlocked {} · exhausted {} · idle {}",
        hive_evidential.converged,
        hive_evidential.deadlocked,
        hive_evidential.exhausted,
        hive_evidential.idle,
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

/// Drive one episode through a real agent CLI.
///
/// Without `--scenario` the room deliberates over a synthetic brief, which
/// measures whether an agent can hold the trace grammar and nothing else.
/// With one it decides a real problem whose answer is recorded, and the
/// independent-vote control is run against the same agents so the deliberation
/// has something to be scored against.
fn live_episode(options: &Options, command: &str) -> Result<(), String> {
    match &options.scenario {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .map_err(|error| format!("could not read {path}: {error}"))?;
            let scenario = Scenario::parse(&text)?;
            live_scenario(options, command, &scenario)
        }
        None => live_synthetic(options, command),
    }
}

/// Deliberate a real problem, then poll the same agents independently.
///
/// Both arms are run `--repeat` times, because a live room is sampled rather
/// than computed and one episode is an anecdote. The trace is printed for the
/// first round only; the rest are counted.
fn live_scenario(options: &Options, command: &str, scenario: &Scenario) -> Result<(), String> {
    let ids = scenario.member_ids();
    // The room size comes from the scenario, so the quorum and budget have to
    // follow it rather than the `--agents` default.
    let policy = EpisodePolicy {
        turn_budget: turn_budget(ids.len()),
        quorum: QuorumPolicy {
            threshold: quorum_threshold(ids.len()),
            ..options.policy.quorum
        },
        ..options.policy
    };

    println!(
        "driving {} episode(s) through {command:?}\n\
         {} members, budget {}, quorum {}\n\nThe brief every member sees:\n{}",
        options.repeat,
        ids.len(),
        policy.turn_budget,
        policy.quorum.threshold,
        scenario.brief(),
    );

    let mut hive_correct = 0_u32;
    let mut hive_decided = 0_u32;
    let mut vote_correct = 0_u32;
    let mut turns_total = 0_u32;
    for round in 0..options.repeat {
        let outcome = live_round(command, scenario, &policy, &ids, round == 0)?;
        if outcome.decided.is_some() {
            hive_decided = hive_decided.saturating_add(1);
        }
        if outcome.decided.as_deref() == Some(scenario.truth.as_str()) {
            hive_correct = hive_correct.saturating_add(1);
        }
        if outcome.voted.as_deref() == Some(scenario.truth.as_str()) {
            vote_correct = vote_correct.saturating_add(1);
        }
        turns_total = turns_total.saturating_add(outcome.turns);
    }

    println!(
        "over {} round(s), answer #{}:\n\
         hive   {} correct, {} decided, {:.1} turns per episode\n\
         vote   {} correct",
        options.repeat,
        scenario.truth,
        hive_correct,
        hive_decided,
        metrics::ratio(u64::from(turns_total), u64::from(options.repeat)),
        vote_correct,
    );
    Ok(())
}

/// What one live round of both arms decided.
struct RoundOutcome {
    /// The topic the deliberation settled on.
    decided: Option<String>,
    /// The topic the independent poll returned, if it was not tied.
    voted: Option<String>,
    /// Turns the deliberation took.
    turns: u32,
}

/// Run one deliberation and one independent poll over the same room.
fn live_round(
    command: &str,
    scenario: &Scenario,
    policy: &EpisodePolicy,
    ids: &[&str],
    keep_trace: bool,
) -> Result<RoundOutcome, String> {
    let mut agents: Vec<LiveAgent> = scenario
        .agents
        .iter()
        .filter_map(|agent| {
            LiveAgent::new(
                &agent.id,
                &agent.role,
                command,
                policy.quorum,
                Scenario::private_brief(agent),
            )
        })
        .collect();
    if agents.len() != ids.len() {
        return Err(format!("could not build agents from {command:?}"));
    }
    let mut participants: Vec<&mut dyn Participant> = agents
        .iter_mut()
        .map(|agent| agent as &mut dyn Participant)
        .collect();

    let wall = std::time::Instant::now();
    let report = drive(
        ids,
        &mut participants,
        policy,
        &scenario.brief(),
        keep_trace,
    )?;
    let wall = wall.elapsed();
    for line in &report.trace {
        println!("{line}");
    }
    let decided = report.decided.as_ref().map(TopicId::as_str);
    println!(
        "\nhive   ended {} on {} after {} turns in {:.0} s — {}",
        report.ending.label(),
        decided.map_or_else(|| "nothing".to_owned(), |topic| format!("#{topic}")),
        report.turns,
        wall.as_secs_f64(),
        verdict(decided, &scenario.truth),
    );

    let picks = live::poll(scenario, command)?;
    for (id, pick) in &picks {
        println!("vote   {id:>10} alone: #{pick}");
    }
    let voted = plurality(&picks);
    match &voted {
        Some(topic) => println!(
            "vote   plurality #{topic} of {} — {}",
            picks.len(),
            verdict(Some(topic.as_str()), &scenario.truth),
        ),
        None => println!("vote   tied, no plurality — no answer"),
    }
    println!();

    Ok(RoundOutcome {
        decided: decided.map(str::to_owned),
        voted,
        turns: report.turns,
    })
}

/// The single option with the most votes, or `None` when the poll is tied.
///
/// A tie is not a decision and must not be reported as one. Resolving it by
/// the order the votes happened to arrive would hand the vote control a win it
/// did not earn, which is exactly the kind of quiet thumb on the scale a
/// benchmark exists to avoid.
fn plurality(picks: &[(String, String)]) -> Option<String> {
    let mut tally: Vec<(&str, u32)> = Vec::new();
    for (_, pick) in picks {
        match tally.iter_mut().find(|(topic, _)| *topic == pick.as_str()) {
            Some(entry) => entry.1 = entry.1.saturating_add(1),
            None => tally.push((pick.as_str(), 1)),
        }
    }
    let most = tally.iter().map(|(_, count)| *count).max()?;
    let mut leaders = tally.iter().filter(|(_, count)| *count == most);
    let leader = leaders.next()?;
    if leaders.next().is_some() {
        return None;
    }
    Some(leader.0.to_owned())
}

/// Whether an arm landed on the recorded answer.
fn verdict(decided: Option<&str>, truth: &str) -> &'static str {
    match decided {
        Some(topic) if topic == truth => "correct",
        Some(_) => "wrong",
        None => "no answer",
    }
}

/// Deliberate the synthetic brief, which has no recorded answer.
fn live_synthetic(options: &Options, command: &str) -> Result<(), String> {
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
            LiveAgent::new(
                id,
                roles.get(index).copied().unwrap_or("teammate"),
                command,
                options.policy.quorum,
                String::new(),
            )
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
fn swarm_compare(options: &Options) -> Result<(), String> {
    if let Some(command) = &options.agent {
        let Some(path) = &options.scenario else {
            return Err("a live federation needs --scenario".to_owned());
        };
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("could not read {path}: {error}"))?;
        let scenario = Scenario::parse(&text)?;
        return live_federation(options, command, &scenario);
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

    let mut siloed = SwarmTotals::default();
    let mut swarmed = SwarmTotals::default();
    let mut free = SwarmTotals::default();
    let mut merged = Aggregate::default();
    let mut vote = Aggregate::default();
    let wall = Instant::now();
    for federation in &federations {
        siloed.add(&run_swarm(
            federation,
            &desk_policy,
            ReferralPolicy::DEFAULT,
            TASK,
            false,
        )?);
        swarmed.add(&run_swarm(
            federation,
            &desk_policy,
            swarm_referrals(),
            TASK,
            false,
        )?);
        free.add(&run_swarm(
            &pooled(federation),
            &desk_policy,
            ReferralPolicy::DEFAULT,
            TASK,
            false,
        )?);
        merged.add_arm(&arms::run_merged(federation, &merged_policy, TASK)?);
        vote.add_arm(&arms::run_federated_vote(federation));
    }
    let wall = wall.elapsed();

    tabulate(
        &siloed,
        &swarmed,
        &free,
        &merged,
        &vote,
        wall,
        federations.len(),
    );
    Ok(())
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
            "{name:<9} {:>7.1}% {:>7} {:>9.1} {:>10.1} {:>9.1}",
            metrics::ratio(u64::from(self.correct), runs) * 100.0,
            self.decided,
            metrics::ratio(self.turns, runs),
            metrics::ratio(self.crossings, runs),
            metrics::ratio(self.stranded, runs),
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
fn live_federation(options: &Options, command: &str, scenario: &Scenario) -> Result<(), String> {
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

    let mut seated = seat_federation(&channels, scenario, command, policy.quorum)?;
    let seats = seated.len();
    println!(
        "driving {} desks through {command:?}\n\
         {} members, budget {} per desk, quorum {}, referrals {} hops\n\n\
         The brief every desk sees:\n{}",
        channels.len(),
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

    let mut members: Vec<&mut dyn swarm::SwarmMember> = seated
        .iter_mut()
        .map(|member| member as &mut dyn swarm::SwarmMember)
        .collect();
    let wall = Instant::now();
    let report = swarm::drive_swarm(
        &channels,
        &mut members,
        &policy,
        swarm_referrals(),
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
        "       {} messages crossed a channel, {} answers arrived too late",
        report.crossings, report.stranded,
    );

    let picks = live::poll(scenario, command)?;
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
    for desk in &first.desks {
        print!("{} overrates #{}  ", desk.name, desk.decoy);
    }
    println!("and #{} is genuinely best\n", first.truth);
}

/// Print one federated episode, channel by channel.
fn trace_swarm(first: &Federation, desk_policy: &EpisodePolicy) -> Result<(), String> {
    let report = run_swarm(first, desk_policy, swarm_referrals(), TASK, true)?;
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
fn tabulate(
    siloed: &SwarmTotals,
    swarmed: &SwarmTotals,
    free: &SwarmTotals,
    merged: &Aggregate,
    vote: &Aggregate,
    wall: std::time::Duration,
    federations: usize,
) {
    println!(
        "{:<9} {:>8} {:>8} {:>9} {:>10} {:>9}",
        "arm", "correct", "decided", "turns", "crossings", "stranded",
    );
    println!("{}", siloed.row("siloed"));
    println!("{}", swarmed.row("swarm"));
    println!("{}", free.row("pooled"));
    println!(
        "{:<9} {:>7.1}% {:>7} {:>9.1} {:>10} {:>9}",
        "merged",
        merged.accuracy(),
        merged.converged,
        merged.turns_per_episode(),
        "—",
        "—",
    );
    println!(
        "{:<9} {:>7.1}% {:>7} {:>9.1} {:>10} {:>9}",
        "vote",
        vote.accuracy(),
        vote.converged,
        vote.turns_per_episode(),
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

/// Build one live agent per member, each knowing which channel it sits on.
fn seat_federation(
    channels: &[swarm::Channel],
    scenario: &Scenario,
    command: &str,
    quorum: QuorumPolicy,
) -> Result<Vec<live::LiveDeskAgent>, String> {
    let mut seated: Vec<live::LiveDeskAgent> = Vec::new();
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
            let Some(live) = LiveAgent::new(
                &agent.id,
                &agent.role,
                command,
                quorum,
                Scenario::private_brief(agent),
            ) else {
                return Err(format!("could not build agents from {command:?}"));
            };
            seated.push(live::LiveDeskAgent::new(
                live,
                channel.name.clone(),
                peers.clone(),
            ));
        }
    }
    Ok(seated)
}
