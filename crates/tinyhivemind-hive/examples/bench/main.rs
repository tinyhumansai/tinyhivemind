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
//! | `--thinking on\|off` | whether the HTTP backend reasons before answering |
//!
//! See `live.rs` and `http.rs` for what the two live backends drive.

mod arms;
mod federation;
mod http;
mod live;
mod metrics;
mod rng;
mod run;
mod scenario;
mod sim;
mod swarm;
mod sweep;

use std::time::Instant;

use tinyhivemind_hive::{
    Directory, DirectoryPolicy, EpisodePolicy, QuorumPolicy, Sequence, directory,
    trace::{TopicId, Trace},
};

use crate::federation::Federation;
use crate::http::{HttpAgent, HttpConfig, HttpDeskAgent, Thinking, Wire};
use crate::live::{AgentPrompt, Backend, LiveAgent};
use crate::metrics::{
    Aggregate, arm_header, arm_row, detail_header, detail_row, json_line, paired_bootstrap,
    paired_diff_line, spearman_milli, wilson,
};
use crate::rng::mix;
use crate::run::{Participant, drive, run_episode, run_episode_with};
use crate::scenario::{Scenario, ScenarioAgent};
use crate::sim::{Expertise, Room, SPECIALIST_COST_UNIT};
use crate::swarm::{Channel, SwarmMember, SwarmReport, pooled, run_swarm};
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
/// Half-width of the individual error on a hidden-profile evaluation, by
/// default.
///
/// Bounded by the same argument [`SWARM_NOISE`] is bounded by, applied to a
/// planted decoy rather than to a desk's shared bias. At the single-room
/// default of ±90 the 90-point gap `HIDDEN_LIFT` opens between the decoy and
/// the true option is swamped: a lay member's argmax lands on the truth often
/// enough that the matched-budget poll solves the profile by accident, and an
/// arm that beats a poll which already wins is measuring nothing. At ±50 the
/// two error bands barely touch, every non-decisive member answers the decoy,
/// and the profile is hidden in the sense Stasser meant. An explicit
/// `--noise` still wins, so the swamped regime can be asked for on purpose.
const HIDDEN_NOISE: u32 = 50;

/// The task every room is given.
const TASK: &str = "We must choose one rollout strategy for a risky migration. Decide together.";

/// Parsed command line.
// A CLI options struct is exactly the shape this pedantic lint warns about
// and exactly the shape a command line is: one independent on/off flag per
// field, not a state machine with exclusive states.
#[allow(clippy::struct_excessive_bools)]
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
    /// How private evaluations are distributed across a room.
    expertise: Expertise,
    /// Whether a specialist's own turn costs more than a lay member's.
    cost: bool,
    /// Turns a member may spend deferring to a topic's expert instead of
    /// arguing outside its own specialty. Read by the deferring arms, and by
    /// the `defer_cap` those arms put in their episode policy.
    defer_cap: u32,
    /// Prior episodes of `hive+` the `ladder+dir` arm earns its directory
    /// from, on the same room.
    history: u32,
    /// Print one flat JSON object per arm, ahead of the tables.
    json: bool,
    /// Per-turn timeout for a live agent process or HTTP request, in seconds.
    timeout: u64,
    /// The HTTP backend's base URL, when seats are driven directly over HTTP
    /// rather than through an agent CLI. Its presence is what selects the
    /// HTTP backend.
    api_base: Option<String>,
    /// The environment variable the HTTP backend's API key is read from.
    api_key_env: String,
    /// The HTTP backend's default model, used by any seat with no
    /// `--seat-model` override.
    model: String,
    /// Which wire format the HTTP backend speaks.
    wire: Wire,
    /// Cost per 1000 tokens, by model name, for the usage table.
    model_cost: Vec<(String, u64)>,
    /// Per-seat model override, by agent id, for the HTTP backend.
    seat_model: Vec<(String, String)>,
    /// Per-seat command override, by agent id, for the CLI backend.
    seat_cmd: Vec<(String, String)>,
    /// The model assigned to a seat the scenario marks as a specialist.
    specialist_model: Option<String>,
    /// Whether the HTTP backend is asked to think before it answers.
    thinking: Thinking,
}

/// What this run does.
enum Mode {
    /// Compare every arm.
    Compare,
    /// Print one episode turn by turn.
    Trace,
    /// Search the policy grid.
    Sweep,
    /// Drive one episode through a real agent CLI or an HTTP backend.
    Live,
    /// Compare several desks solving one problem across channels.
    Swarm,
    /// Run the hidden self-check over `wilson`, `paired_bootstrap` and
    /// `spearman_milli` and exit.
    StatsCheck,
}

impl Options {
    /// The options every mode starts from before a flag overrides one.
    fn defaults() -> Self {
        Self {
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
            expertise: Expertise::Uniform,
            cost: false,
            defer_cap: 1,
            history: 3,
            json: false,
            timeout: 180,
            api_base: None,
            api_key_env: "LADDER_API_KEY".to_owned(),
            model: "flash".to_owned(),
            wire: Wire::OpenAi,
            model_cost: Vec::new(),
            seat_model: Vec::new(),
            seat_cmd: Vec::new(),
            specialist_model: None,
            thinking: Thinking::On,
        }
    }

    /// Read the command line, falling back to defaults.
    fn parse() -> Self {
        let mut options = Self::defaults();
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
        // A hidden profile has its own noise default for the same reason, and
        // by the same rule: an explicit `--noise` wins.
        if args.iter().any(|argument| argument == "--hidden-profile")
            && flag_number(&args, "--noise").is_none()
        {
            options.noise = HIDDEN_NOISE;
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
                        options.agent = Some(command);
                        // `--swarm --agent-cmd` drives a federation rather than
                        // one room, so the swarm mode keeps the floor.
                        if !matches!(options.mode, Mode::Swarm) {
                            options.mode = Mode::Live;
                        }
                    }
                }
                "--scenario" => options.scenario = args.next(),
                "--repeat" => options.repeat = next_number(&mut args).unwrap_or(1).max(1),
                "--json" => options.json = true,
                "--stats-check" => options.mode = Mode::StatsCheck,
                // Everything below is either the expertise surface or the
                // live-backend one: a CLI or HTTP seat, per-seat overrides,
                // and the usage table. Split into their own functions so
                // `parse` itself stays under the line budget clippy holds
                // every function to.
                _ => {
                    apply_expertise_flag(&mut options, &flag, &mut args);
                    apply_live_flag(&mut options, &flag, &mut args);
                }
            }
        }
        options
    }
}

/// Apply one of `--specialists`, `--hidden-profile`, `--defer-cap` or
/// `--cost-tiers` to `options`, or do nothing for a flag it does not
/// recognise.
fn apply_expertise_flag(
    options: &mut Options,
    flag: &str,
    args: &mut impl Iterator<Item = String>,
) {
    match flag {
        "--specialists" => {
            let count = usize::try_from(next_number(args).unwrap_or(0)).unwrap_or(0);
            options.expertise = Expertise::Specialists { count };
        }
        "--hidden-profile" => options.expertise = Expertise::HiddenProfile,
        "--defer-cap" => options.defer_cap = next_number(args).unwrap_or(1).max(1),
        "--history" => options.history = next_number(args).unwrap_or(3),
        "--cost-tiers" => options.cost = true,
        _ => {}
    }
}

/// Apply one of the live-backend flags (`--timeout` through
/// `--specialist-model`) to `options`, or do nothing for a flag it does not
/// recognise.
fn apply_live_flag(options: &mut Options, flag: &str, args: &mut impl Iterator<Item = String>) {
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
        _ => {}
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

/// The tuned policy with the folded transactive-memory directory switched on.
///
/// One field moves. `directory: Some(DirectoryPolicy::DEFAULT)` is what makes
/// [`BidReason::Knows`] reachable at all: without it there is no contested
/// topic and no holder to promote, so the attention market routes on recency
/// and trace importance exactly as it did before. Nothing else about the
/// episode changes, which is what lets the difference between this arm and
/// `hive+` be attributed to the directory rather than to a second knob.
///
/// [`BidReason::Knows`]: tinyhivemind_hive::BidReason::Knows
fn knowing_policy(tuned: &EpisodePolicy) -> EpisodePolicy {
    EpisodePolicy {
        directory: Some(DirectoryPolicy::DEFAULT),
        ..*tuned
    }
}

/// The tuned policy with `!defer` bounded at `cap`, and no directory.
///
/// This is the honest control for the deferring arm: a room where members may
/// stand aside on a topic that is not theirs, but where nothing folds a
/// directory to route the vacated turn anywhere in particular. If `!defer`
/// pays for itself only in the presence of a directory, that is worth knowing
/// separately from whether it pays for itself at all.
fn deferring_policy(tuned: &EpisodePolicy, cap: u32) -> EpisodePolicy {
    EpisodePolicy {
        defer_cap: Some(cap.max(1)),
        ..*tuned
    }
}

/// Both mechanisms at once: the directory folded, and `!defer` bounded.
///
/// This is the arrangement `docs/specs/expert-delegation.md` describes end to
/// end — a member says "not mine", that promotes the topic to the contested
/// one, and the directory decides who the vacated turn goes to.
fn knowing_deferring_policy(tuned: &EpisodePolicy, cap: u32) -> EpisodePolicy {
    EpisodePolicy {
        directory: Some(DirectoryPolicy::DEFAULT),
        defer_cap: Some(cap.max(1)),
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
            Room::generate_with(
                mix(options.seed, u64::from(index)),
                options.agents,
                options.topics,
                options.noise,
                options.expertise,
                options.cost,
            )
        })
        .collect();

    match &options.mode {
        // Handled above, before the single-desk rooms were generated.
        Mode::Swarm | Mode::StatsCheck => Ok(()),
        Mode::Compare => compare(options, &rooms),
        Mode::Trace => trace(&rooms, &options.policy),
        Mode::Sweep => sweep_policies(options, &rooms),
        Mode::Live => live_episode(options),
    }
}

/// Run every arm over the same rooms and print the comparison.
fn compare(options: &Options, rooms: &[Room]) -> Result<(), String> {
    let tuned = options.policy;
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

    let (totals, wall) = run_arms(options, rooms)?;
    let arms: [(&str, &Aggregate); 10] = [
        ("ladder", &totals.ladder),
        ("vote", &totals.vote),
        ("hive", &totals.hive_default),
        ("hive+", &totals.hive_tuned),
        ("hive+ref", &totals.hive_refuting),
        ("hive+ev", &totals.hive_evidential),
        // Appended rather than interleaved: the six rows above are the
        // published table, and the paired-bootstrap seed below is derived
        // from an arm's index in this list.
        ("hive+dir", &totals.hive_knowing),
        ("hive+defer", &totals.hive_deferring),
        ("hive+dir+defer", &totals.hive_both),
        ("ladder+dir", &totals.ladder_directed),
    ];

    if options.json {
        for (name, arm) in arms {
            println!("{}", json_line(name, arm));
        }
        if options.cost {
            println!("{}", json_line("all-reasoning", &totals.all_reasoning));
        }
    }

    println!("{}", arm_header());
    for (name, arm) in arms {
        println!("{}", arm_row(name, arm));
    }

    println!("\n{}", detail_header());
    for (name, arm) in arms {
        println!("{}", detail_row(name, arm));
    }
    for (index, (name, arm)) in arms.iter().filter(|(name, _)| *name != "vote").enumerate() {
        let seed = mix(options.seed, 0xB007_57AA_u64.wrapping_add(index as u64));
        if let Some(line) = paired_diff_line(name, arm, &totals.vote, seed, 2000) {
            println!("{line}");
        }
    }

    if options.cost {
        cost_table(&[
            ("vote", &totals.vote),
            ("ladder", &totals.ladder),
            ("ladder+dir", &totals.ladder_directed),
            ("hive+", &totals.hive_tuned),
            ("hive+cost", &totals.hive_both),
            ("all-reasoning", &totals.all_reasoning),
        ]);
    }

    endings(&totals);
    println!(
        "library time {:.1} ms over {} steps ({:.0} ns/step, {:.0} episodes/s)",
        totals.hive_tuned.library_time.as_secs_f64() * 1_000.0,
        totals.hive_tuned.step_calls,
        totals.hive_tuned.nanos_per_step(),
        totals.hive_tuned.episodes_per_second(),
    );
    println!(
        "wall clock {:.1} ms for {} rooms across every arm",
        wall.as_secs_f64() * 1_000.0,
        rooms.len(),
    );
    Ok(())
}

/// Every arm's running totals over one sample of rooms.
#[derive(Default)]
struct Totals {
    /// A deliberation at the crate's own default policy.
    hive_default: Aggregate,
    /// The same at the tuned policy the sweep picked.
    hive_tuned: Aggregate,
    /// The tuned policy with `refutation_cap` on.
    hive_refuting: Aggregate,
    /// The same, plus `require_evidential`.
    hive_evidential: Aggregate,
    /// The tuned policy with the folded directory on.
    hive_knowing: Aggregate,
    /// The tuned policy with `!defer` bounded, and no directory.
    hive_deferring: Aggregate,
    /// Both delegation mechanisms at once.
    hive_both: Aggregate,
    /// The tuned policy in a room that puts every seat on the expensive
    /// tier. Only filled under `--cost-tiers`.
    all_reasoning: Aggregate,
    /// The matched-budget independent poll.
    vote: Aggregate,
    /// One responder off the real ladder, chosen without information.
    ladder: Aggregate,
    /// The same ladder, given a directory the room earned.
    ladder_directed: Aggregate,
}

/// Run every arm over the same rooms, and say how long the whole sample took.
///
/// # Errors
///
/// Returns the library's own error text from any arm.
fn run_arms(options: &Options, rooms: &[Room]) -> Result<(Totals, std::time::Duration), String> {
    let tuned = options.policy;
    let default = default_policy();
    let refuting = refuting_policy(&tuned);
    let evidential = evidential_policy(&tuned);
    let knowing = knowing_policy(&tuned);
    let deferring = deferring_policy(&tuned, options.defer_cap);
    let both = knowing_deferring_policy(&tuned, options.defer_cap);
    let mut totals = Totals::default();
    let wall = Instant::now();
    for (index, room) in rooms.iter().enumerate() {
        totals
            .hive_default
            .add(&run_episode(room, &default, TASK, false)?);
        totals
            .hive_tuned
            .add(&run_episode(room, &tuned, TASK, false)?);
        totals
            .hive_refuting
            .add(&run_episode(room, &refuting, TASK, false)?);
        totals
            .hive_evidential
            .add(&run_episode(room, &evidential, TASK, false)?);
        // The three delegation arms. Only the deferring two hand their members
        // a non-zero cap, so `hive+dir` differs from `hive+` in the policy
        // field alone and in nothing a participant does.
        totals
            .hive_knowing
            .add(&run_episode(room, &knowing, TASK, false)?);
        totals.hive_deferring.add(&run_episode_with(
            room,
            &deferring,
            TASK,
            false,
            options.defer_cap,
        )?);
        totals.hive_both.add(&run_episode_with(
            room,
            &both,
            TASK,
            false,
            options.defer_cap,
        )?);
        if options.cost {
            totals.all_reasoning.add(&run_episode(
                &room.at_cost(SPECIALIST_COST_UNIT),
                &tuned,
                TASK,
                false,
            )?);
        }
        let seed = mix(options.seed, u64::try_from(index).unwrap_or(0));
        totals.ladder.add_arm(&arms::run_ladder(room, seed)?);
        let earned = earn_directory(room, &tuned, options.history, mix(seed, 0x6869_7374))?;
        totals
            .ladder_directed
            .add_arm(&arms::run_ladder_directed(room, &earned, seed)?);
        // The control is given the whole budget, which is more turns than the
        // deliberation actually spends. It is the arm to beat, so it gets
        // every advantage.
        totals
            .vote
            .add_arm(&arms::run_vote(room, tuned.turn_budget));
    }
    Ok((totals, wall.elapsed()))
}

/// Print how each deliberating arm's episodes ended.
fn endings(totals: &Totals) {
    println!();
    for (name, arm) in [
        ("hive ", &totals.hive_default),
        ("hive+", &totals.hive_tuned),
        ("hive+ref", &totals.hive_refuting),
        ("hive+ev", &totals.hive_evidential),
        ("hive+dir+defer", &totals.hive_both),
    ] {
        println!(
            "{name} endings: converged {} · deadlocked {} · exhausted {} · idle {}",
            arm.converged, arm.deadlocked, arm.exhausted, arm.idle,
        );
    }
}

/// Earn a directory for one room by running `--history` prior episodes of
/// `hive+` on it and folding the whole record once.
///
/// The `ladder+dir` arm is not allowed to be handed the answer. A directory
/// invented by the harness, or read off `Room::experts`, would measure the
/// harness rather than the mechanism, so this earns one the only way the
/// library offers: it deliberates the same room several times and folds what
/// those transcripts recorded.
///
/// The several episodes are *concatenated with renumbered sequences* and
/// folded once, rather than folded separately and merged by summing weights.
/// Both were available; this one is chosen because it is the fold the library
/// actually defines. Summing weights across separate folds would double-count
/// the `WEIGHT_CEILING` clamp and would apply each episode's decay from its
/// own end, so a member's total would depend on how the history happened to
/// be cut into episodes. One fold over one renumbered record has one decay
/// origin and one clamp. It also means [`DirectoryPolicy::DEFAULT`]'s
/// `window` of 30 sequences applies to the *whole* history: past about four
/// episodes of a five-member room the earliest ones fall out of window, which
/// is why `--history 5` is not simply a stronger `--history 3`.
///
/// Each replay is [`Room::resampled`] — the same members and the same private
/// evaluations, with only the noncompliance draw reseeded — because the
/// simulated participants are otherwise deterministic and replaying a room
/// would produce the same transcript N times. `seed` is this room's own
/// stream, so two rooms do not share a resampling.
///
/// # Errors
///
/// Returns the library's own error text from an episode or from the fold.
fn earn_directory(
    room: &Room,
    tuned: &EpisodePolicy,
    history: u32,
    seed: u64,
) -> Result<Directory, String> {
    let mut record: Vec<Trace> = Vec::new();
    let mut offset = 0_u64;
    for episode in 0..history {
        let seed = mix(seed, u64::from(episode));
        let report = run_episode(&room.resampled(seed), tuned, TASK, false)?;
        let mut highest = 0_u64;
        for trace in report.traces {
            highest = highest.max(trace.sequence.0);
            record.push(shift(trace, offset));
        }
        offset = offset.saturating_add(highest);
    }
    let at = Sequence(offset);
    directory(&record, at, &DirectoryPolicy::DEFAULT, &[]).map_err(|error| error.to_string())
}

/// Move one trace, and every sequence it names, forward by `offset`.
///
/// A citation names a sequence, so renumbering a trace without renumbering
/// what it cites would silently break every credibility term in the fold —
/// the citation would land on whatever the earlier episode happened to have
/// at that number.
fn shift(trace: Trace, offset: u64) -> Trace {
    Trace {
        sequence: Sequence(trace.sequence.0.saturating_add(offset)),
        target: trace
            .target
            .map(|target| Sequence(target.0.saturating_add(offset))),
        cites: trace
            .cites
            .into_iter()
            .map(|cited| Sequence(cited.0.saturating_add(offset)))
            .collect(),
        ..trace
    }
}

/// Print what each arm spent, and what its right answers cost.
///
/// Only under `--cost-tiers`, where a specialist's turn is charged ten times
/// a lay member's and the question stops being "which arm is most accurate"
/// and becomes "which arm is most accurate per unit spent". `correct/kU` is
/// right answers per thousand cost units: an arm that buys two more points of
/// accuracy by putting every seat on the expensive tier should be visible
/// here as having bought them badly.
fn cost_table(arms: &[(&str, &Aggregate)]) {
    println!(
        "\n{:<15}{:>11}{:>10}{:>14}",
        "arm", "correct %", "cost/ep", "correct/kU",
    );
    for (name, totals) in arms {
        println!(
            "{:<15}{:>11.1}{:>10.2}{:>14.2}",
            name,
            totals.accuracy(),
            totals.cost_per_episode(),
            totals.accuracy_per_kilo_unit(),
        );
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
fn http_config(options: &Options) -> Result<HttpConfig, String> {
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
fn backend_label(options: &Options) -> String {
    match &options.api_base {
        Some(base) => format!("{base} (model {}, {:?} wire)", options.model, options.wire),
        None => options.agent.clone().unwrap_or_default(),
    }
}

/// Whether the scenario marks `agent` as a specialist `--specialist-model`
/// should seat instead of the backend's default model.
///
/// True for a member the scenario names as a reasoning-tier seat, or as an
/// expert on at least one named area — either is a claim the scenario itself
/// makes about the room, not one the harness infers from a seat's answers.
fn is_specialist(agent: &ScenarioAgent) -> bool {
    agent.tier.as_deref() == Some("reasoning") || !agent.expert_on.is_empty()
}

/// The model an HTTP seat for `agent` should run under.
///
/// A `--seat-model` override wins outright; otherwise a specialist seat runs
/// under `--specialist-model` when one was given, and every other seat runs
/// under the backend's default `--model`.
fn seat_model<'a>(options: &'a Options, agent: &ScenarioAgent) -> &'a str {
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
fn seat_command<'a>(options: &'a Options, agent: &ScenarioAgent) -> Result<&'a str, String> {
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
fn poll_backend(options: &Options) -> Result<Backend, String> {
    if options.api_base.is_some() {
        return Ok(Backend::Http {
            config: http_config(options)?,
            model: options.model.clone(),
        });
    }
    let command = options
        .agent
        .clone()
        .ok_or_else(|| "no --agent-cmd or --api-base given".to_owned())?;
    Ok(Backend::Cli(command))
}

/// One HTTP seat's usage, kept alongside its boxed participant so it can be
/// read back after the episode has finished driving.
struct SeatUsage {
    /// The seat's canonical agent id.
    agent_id: String,
    /// The model this seat ran under.
    model: String,
    /// The tier the scenario said this seat stands in for, if it said.
    tier: Option<String>,
    /// The shared handle onto its running total.
    handle: crate::http::UsageHandle,
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
fn print_usage(options: &Options, seats: &[SeatUsage]) {
    if seats.is_empty() {
        return;
    }
    let mut total_tokens = 0_u64;
    let mut total_cost = 0_u64;
    for seat in seats {
        let usage = *seat.handle.borrow();
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
        let usage = *seat.handle.borrow();
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

/// Print how the round used the member who actually held the deciding fact:
/// whether the scenario's `truth_expert` spoke at all, whether it spoke
/// *before* the room recorded its decision, and whether the winning commit's
/// citation chain reaches anything that member said.
///
/// Returns `(spoke before the commit, cited by the commit)` so the caller can
/// count both across `--repeat` rounds.
///
/// Mirrors what `run_episode` reports for a simulated room's own specialist
/// or decisive member; a live scenario has no `Room::experts` to read back,
/// so this reads `first_spoke` and the episode's own traces, which `drive`
/// already builds from turns it saw, against the id the scenario itself
/// names.
fn print_expert(scenario: &Scenario, report: &crate::run::EpisodeReport) -> (bool, bool) {
    let Some(expert) = &scenario.truth_expert else {
        return (false, false);
    };
    // Every turn but the last is before the commit, and the commit turn is
    // the last one an episode that converged ever took.
    let commit_at = report.turns.saturating_sub(1);
    let spoke = report
        .first_spoke
        .iter()
        .find(|(id, _)| id == expert)
        .map(|(_, turn)| *turn);
    let before = spoke.is_some_and(|turn| turn < commit_at);
    match spoke {
        Some(turn) => println!(
            "hive   expert @{expert} first spoke at turn {turn} ({} the commit)",
            if before { "before" } else { "not before" },
        ),
        None => println!("hive   expert @{expert} never spoke"),
    }
    // Only an episode that actually recorded a decision has a winning commit
    // whose citation chain there is anything to walk.
    let cited = match &report.decided {
        Some(topic) => {
            let cited = commit_reaches(&report.traces, topic, expert);
            println!(
                "hive   the winning commit's citation chain {} @{expert}",
                if cited { "reaches" } else { "does not reach" },
            );
            cited
        }
        None => false,
    };
    println!("hive   {} turn(s) spent on !defer", report.defers);
    (before, cited)
}

/// Whether the `!commit` for `topic` cites, directly or through any chain of
/// citations, a message `expert` authored.
///
/// This is the question a scenario's `truth_expert` exists to make askable:
/// a room can reach the right answer while having ignored the member who
/// held the fact, and a room that reached it *through* that member has done
/// something the poll cannot. Walked breadth-first over the traces the
/// episode's own journal produced, with a visited set, so a citation cycle
/// terminates.
fn commit_reaches(traces: &[Trace], topic: &TopicId, expert: &str) -> bool {
    let Some(commit) = traces.iter().find(|trace| {
        trace.kind == tinyhivemind_hive::TraceKind::Commit && trace.topic.as_ref() == Some(topic)
    }) else {
        return false;
    };
    let mut frontier: Vec<Sequence> = commit.cites.clone();
    frontier.extend(commit.target);
    let mut seen: Vec<Sequence> = Vec::new();
    while let Some(at) = frontier.pop() {
        if seen.contains(&at) {
            continue;
        }
        seen.push(at);
        for trace in traces.iter().filter(|trace| trace.sequence == at) {
            if trace.agent_id() == Some(expert) {
                return true;
            }
            frontier.extend(trace.cites.iter().copied());
            frontier.extend(trace.target);
        }
    }
    false
}

/// Print who the scenario says the winning option was somebody's call, and
/// whether that member actually backed it.
///
/// `expert:` on an option is the scenario's own claim that one member's
/// judgment on that specific call is worth deferring to. It is never shown
/// to the room — telling the room who to trust per option would hand it the
/// answer — so it is only ever read back here, after the round has decided.
fn print_option_expert(
    scenario: &Scenario,
    decided: Option<&str>,
    report: &crate::run::EpisodeReport,
) {
    let Some(decided) = decided else {
        return;
    };
    let Some(expert) = scenario
        .options
        .iter()
        .find(|option| option.id == decided)
        .and_then(|option| option.expert.as_deref())
    else {
        return;
    };
    let backed = report.traces.iter().any(|trace| {
        matches!(
            trace.kind,
            tinyhivemind_hive::TraceKind::Propose | tinyhivemind_hive::TraceKind::Support
        ) && trace.topic.as_ref().map(TopicId::as_str) == Some(decided)
            && trace.agent_id() == Some(expert)
    });
    println!(
        "hive   #{decided} is @{expert}'s call — they {} it",
        if backed { "backed" } else { "never backed" },
    );
}

/// Build one participant for a scenario member, honoring per-seat overrides.
///
/// # Errors
///
/// Returns a message naming the missing command, key, or base.
fn seat_participant(
    options: &Options,
    agent: &ScenarioAgent,
    quorum: QuorumPolicy,
) -> Result<(Box<dyn Participant>, Option<SeatUsage>), String> {
    let private = Scenario::private_brief(agent);
    if options.api_base.is_some() {
        let config = http_config(options)?;
        let model = seat_model(options, agent).to_owned();
        let prompt = AgentPrompt::new(&agent.id, &agent.role, quorum, private);
        let http_agent = HttpAgent::new(prompt, config, model.clone());
        let usage = SeatUsage {
            agent_id: agent.id.clone(),
            model,
            tier: agent.tier.clone(),
            handle: http_agent.usage_handle(),
        };
        return Ok((Box::new(http_agent), Some(usage)));
    }
    let command = seat_command(options, agent)?;
    let live = LiveAgent::new(
        &agent.id,
        &agent.role,
        command,
        quorum,
        private,
        options.timeout,
    )
    .ok_or_else(|| format!("could not build agent from {command:?}"))?;
    Ok((Box::new(live), None))
}

/// Build one participant for the synthetic (no-scenario) room.
///
/// Per-seat overrides only make sense against a named scenario member, so the
/// synthetic room only ever runs the backend's default command or model.
///
/// # Errors
///
/// Returns a message naming the missing command, key, or base.
fn seat_synthetic(
    options: &Options,
    id: &str,
    role: &str,
    quorum: QuorumPolicy,
) -> Result<(Box<dyn Participant>, Option<SeatUsage>), String> {
    if options.api_base.is_some() {
        let config = http_config(options)?;
        let model = options.model.clone();
        let prompt = AgentPrompt::new(id, role, quorum, String::new());
        let http_agent = HttpAgent::new(prompt, config, model.clone());
        let usage = SeatUsage {
            agent_id: id.to_owned(),
            model,
            tier: None,
            handle: http_agent.usage_handle(),
        };
        return Ok((Box::new(http_agent), Some(usage)));
    }
    let command = options
        .agent
        .as_deref()
        .ok_or_else(|| "no --agent-cmd or --api-base given".to_owned())?;
    let live = LiveAgent::new(id, role, command, quorum, String::new(), options.timeout)
        .ok_or_else(|| format!("could not build agent from {command:?}"))?;
    Ok((Box::new(live), None))
}

/// Drive one episode through a real agent CLI or an HTTP backend.
///
/// Without `--scenario` the room deliberates over a synthetic brief, which
/// measures whether an agent can hold the trace grammar and nothing else.
/// With one it decides a real problem whose answer is recorded, and the
/// independent-vote control is run against the same agents so the deliberation
/// has something to be scored against.
fn live_episode(options: &Options) -> Result<(), String> {
    match &options.scenario {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .map_err(|error| format!("could not read {path}: {error}"))?;
            let scenario = Scenario::parse(&text)?;
            live_scenario(options, &scenario)
        }
        None => live_synthetic(options),
    }
}

/// Deliberate a real problem, then poll the same agents independently.
///
/// Both arms are run `--repeat` times, because a live room is sampled rather
/// than computed and one episode is an anecdote. The trace is printed for the
/// first round only; the rest are counted.
fn live_scenario(options: &Options, scenario: &Scenario) -> Result<(), String> {
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
        "driving {} episode(s) through {}\n\
         {} members, budget {}, quorum {}\n\nThe brief every member sees:\n{}",
        options.repeat,
        backend_label(options),
        ids.len(),
        policy.turn_budget,
        policy.quorum.threshold,
        scenario.brief(),
    );

    let mut hive_correct = 0_u32;
    let mut hive_decided = 0_u32;
    let mut vote_correct = 0_u32;
    let mut turns_total = 0_u32;
    let mut expert_early = 0_u32;
    let mut expert_cited = 0_u32;
    let mut defers_total = 0_u32;
    for round in 0..options.repeat {
        let outcome = live_round(options, scenario, &policy, &ids, round == 0)?;
        if outcome.expert_early {
            expert_early = expert_early.saturating_add(1);
        }
        if outcome.expert_cited {
            expert_cited = expert_cited.saturating_add(1);
        }
        defers_total = defers_total.saturating_add(outcome.defers);
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
         hive   expert spoke before the commit in {} round(s), cited by the commit in {}\n\
         hive   {} turn(s) spent on !defer across every round\n\
         vote   {} correct",
        options.repeat,
        scenario.truth,
        hive_correct,
        hive_decided,
        metrics::ratio(u64::from(turns_total), u64::from(options.repeat)),
        expert_early,
        expert_cited,
        defers_total,
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
    /// Whether the scenario's `truth_expert` spoke before the commit.
    expert_early: bool,
    /// Whether the winning commit's citation chain reaches that member.
    expert_cited: bool,
    /// Turns the round spent on `!defer`.
    defers: u32,
}

/// Run one deliberation and one independent poll over the same room.
fn live_round(
    options: &Options,
    scenario: &Scenario,
    policy: &EpisodePolicy,
    ids: &[&str],
    keep_trace: bool,
) -> Result<RoundOutcome, String> {
    let mut agents: Vec<Box<dyn Participant>> = Vec::new();
    let mut usage_seats: Vec<SeatUsage> = Vec::new();
    for agent in &scenario.agents {
        let (participant, usage) = seat_participant(options, agent, policy.quorum)?;
        agents.push(participant);
        if let Some(usage) = usage {
            usage_seats.push(usage);
        }
    }
    if agents.len() != ids.len() {
        return Err("could not seat every scenario member".to_owned());
    }
    // A plain loop rather than `.iter_mut().map(...).collect()`: collecting
    // trait-object references straight out of a `Vec<Box<dyn Participant>>`
    // makes dropck conservatively extend `agents`' borrow to the end of the
    // function, since it cannot see that the reference never outlives the
    // `drive` call below.
    let mut participants: Vec<&mut dyn Participant> = Vec::new();
    for agent in &mut agents {
        participants.push(agent.as_mut());
    }

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
    let (expert_early, expert_cited) = print_expert(scenario, &report);
    print_option_expert(scenario, decided, &report);
    print_usage(options, &usage_seats);

    let backend = poll_backend(options)?;
    let picks = live::poll(scenario, &backend)?;
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
        expert_early,
        expert_cited,
        defers: report.defers,
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
fn live_synthetic(options: &Options) -> Result<(), String> {
    let room = Room::generate(options.seed, options.agents, options.topics, options.noise);
    let ids = room.member_ids();
    let roles = [
        "planner, who proposes concrete options",
        "critic, who looks for the weakness in a proposal",
        "archivist, who supplies precedent and evidence",
        "scout, who looks for the option nobody has raised",
        "auditor, who checks a decision against the constraints",
    ];
    let mut agents: Vec<Box<dyn Participant>> = Vec::new();
    let mut usage_seats: Vec<SeatUsage> = Vec::new();
    for (index, id) in ids.iter().enumerate() {
        let role = roles.get(index).copied().unwrap_or("teammate");
        let (participant, usage) = seat_synthetic(options, id, role, options.policy.quorum)?;
        agents.push(participant);
        if let Some(usage) = usage {
            usage_seats.push(usage);
        }
    }
    if agents.len() != ids.len() {
        return Err("could not seat every synthetic member".to_owned());
    }
    // A plain loop rather than `.iter_mut().map(...).collect()`: collecting
    // trait-object references straight out of a `Vec<Box<dyn Participant>>`
    // makes dropck conservatively extend `agents`' borrow to the end of the
    // function, since it cannot see that the reference never outlives the
    // `drive` call below.
    let mut participants: Vec<&mut dyn Participant> = Vec::new();
    for agent in &mut agents {
        participants.push(agent.as_mut());
    }

    println!("driving one episode through {}\n", backend_label(options));
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
    print_usage(options, &usage_seats);
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
    let mut members: Vec<&mut dyn SwarmMember> = Vec::new();
    for member in &mut seated {
        members.push(member.as_mut());
    }
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
        "       {} messages crossed a channel, {} answers arrived too late, {} turn(s) on !defer",
        report.crossings, report.stranded, report.defers,
    );
    print_usage(options, &usage_seats);

    let backend = poll_backend(options)?;
    let picks = live::poll(scenario, &backend)?;
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
