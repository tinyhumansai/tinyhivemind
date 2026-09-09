//! Command-line parsing for the bench binary.
//!
//! Split out of `main.rs` because a CLI surface this wide — the room and
//! federation knobs, both live backends, the delegation and check-arm caps —
//! is a module in its own right, not a line-count problem. [`Options`] is the
//! parsed argument set every other module reads; [`Mode`] is what `--sweep`,
//! `--trace`, `--swarm` and friends pick between. See `main.rs`'s crate doc
//! for the flag table itself.

use crate::context::{Compaction, ContextBudget, FOLD_FIDELITY};
use crate::http::{Thinking, Wire};
use crate::policy::tuned_policy;
use crate::sim::Expertise;
use tinyhivemind_hive::{DirectoryPolicy, EpisodePolicy};

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
/// Parsed command line.
// A CLI options struct is exactly the shape this pedantic lint warns about
// and exactly the shape a command line is: one independent on/off flag per
// field, not a state machine with exclusive states.
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct Options {
    /// Rooms to simulate.
    pub(crate) episodes: u32,
    /// Members per room.
    pub(crate) agents: usize,
    /// Options on offer.
    pub(crate) topics: usize,
    /// Half-width of the error on each private evaluation.
    pub(crate) noise: u32,
    /// Room generator seed.
    pub(crate) seed: u64,
    /// The policy every mode but the sweep runs at.
    pub(crate) policy: EpisodePolicy,
    /// What this run does.
    pub(crate) mode: Mode,
    /// A real problem for the live room, if one was given.
    pub(crate) scenario: Option<String>,
    /// How many times to run a live scenario.
    pub(crate) repeat: u32,
    /// Desks in a federation.
    pub(crate) desks: usize,
    /// Members on each desk of a federation.
    pub(crate) per_desk: usize,
    /// How much a desk overrates its own decoy.
    pub(crate) bias: i32,
    /// Whether to print a transcript as well as the totals.
    pub(crate) trace: bool,
    /// The agent command, when one was given.
    pub(crate) agent: Option<String>,
    /// How private evaluations are distributed across a room.
    pub(crate) expertise: Expertise,
    /// Room sizes the scale sweep walks.
    pub(crate) sizes: Vec<usize>,
    /// Horizon lengths `--stages` sweeps, in stages. Empty takes the default
    /// ladder in [`crate::horizon::DEFAULT_HORIZONS`].
    pub(crate) horizons: Vec<usize>,
    /// Task widths `--facets` sweeps, in facets. Empty takes the default
    /// ladder in [`crate::variety::DEFAULT_SPREADS`].
    pub(crate) facets: Vec<usize>,
    /// Whether each facet of a `--facets` task has an owner that reads it far
    /// more tightly than anybody else.
    ///
    /// Off by default, so the sweep's own baseline measures the division of
    /// labour and nothing else: every member is equally competent and the arms
    /// differ only in how the load is split. `--roles` adds the second
    /// mechanism on top, which is what lets the two be told apart.
    pub(crate) roles: bool,
    /// Whether a specialist's own turn costs more than a lay member's.
    pub(crate) cost: bool,
    /// Whether a member's first turn, while the room is still blind, is a
    /// deposit rather than a position. See `sim.rs`.
    pub(crate) blind_evidence: bool,
    /// Turns a member may spend deferring to a topic's expert instead of
    /// arguing outside its own specialty. Read by the deferring arms, and by
    /// the `defer_cap` those arms put in their episode policy.
    pub(crate) defer_cap: u32,
    /// Turns a member may spend asking one peer for a second reading before
    /// committing to a position. Read by the two aside arms, which differ from
    /// each other only in who may read the answer. `0` turns both off, and
    /// makes them bit-identical to `hive+`.
    pub(crate) aside_cap: u32,
    /// Turns one round may authorize concurrently, for the `hive+wide` arm.
    ///
    /// Every published arm runs at `policy::SEQUENTIAL`, so this changes only
    /// the concurrency arm and no recorded number moves. `0` makes `hive+wide`
    /// bit-identical to `hive+`, the discipline every other cap here follows.
    pub(crate) round_width: u32,
    /// Rows each member's context window holds. `0` disables the window model
    /// entirely, which is the default and is bit-identical to a build without
    /// it.
    pub(crate) context: usize,
    /// How hard the middle of that window is discounted, `0.0..=1.0`.
    pub(crate) rot: f64,
    /// What a summarised row is worth under [`Compaction::Fold`], read by the
    /// `solo+fold` arm of `--stages`. Ignored everywhere else.
    ///
    /// [`Compaction::Fold`]: crate::context::Compaction::Fold
    pub(crate) fidelity: f64,
    /// Private rows one member may write **off the floor**, read by
    /// `hive+rounds`.
    ///
    /// Separate from `aside_cap` because it bounds a different resource: an
    /// on-floor check spends the room's turns, of which there are a handful,
    /// while an off-floor row spends a model call, of which a host may buy as
    /// many as it will pay for. Sharing one number would understate the
    /// mechanism and misprice the comparison. `0` disables the exchange
    /// entirely, leaving `hive+rounds` bit-identical to `hive+`.
    pub(crate) exchange_cap: u32,
    /// Prior episodes of `hive+` the `ladder+dir` arm earns its directory
    /// from, on the same room.
    pub(crate) history: u32,
    /// Print one flat JSON object per arm, ahead of the tables.
    pub(crate) json: bool,
    /// Per-turn timeout for a live agent process or HTTP request, in seconds.
    pub(crate) timeout: u64,
    /// The HTTP backend's base URL, when seats are driven directly over HTTP
    /// rather than through an agent CLI. Its presence is what selects the
    /// HTTP backend.
    pub(crate) api_base: Option<String>,
    /// The environment variable the HTTP backend's API key is read from.
    pub(crate) api_key_env: String,
    /// The HTTP backend's default model, used by any seat with no
    /// `--seat-model` override.
    pub(crate) model: String,
    /// Which wire format the HTTP backend speaks.
    pub(crate) wire: Wire,
    /// Cost per 1000 tokens, by model name, for the usage table.
    pub(crate) model_cost: Vec<(String, u64)>,
    /// Per-seat model override, by agent id, for the HTTP backend.
    pub(crate) seat_model: Vec<(String, String)>,
    /// Per-seat command override, by agent id, for the CLI backend.
    pub(crate) seat_cmd: Vec<(String, String)>,
    /// The model assigned to a seat the scenario marks as a specialist.
    pub(crate) specialist_model: Option<String>,
    /// Whether the HTTP backend is asked to think before it answers.
    pub(crate) thinking: Thinking,
}

/// What this run does.
impl Options {
    /// The window every member reads through, assembled from the three flags
    /// that describe it.
    ///
    /// `--context 0` is [`ContextBudget::UNBOUNDED`], which every read path
    /// short-circuits on, so a run that does not ask for a window is
    /// bit-identical to one built before the model existed.
    pub(crate) fn budget(&self) -> ContextBudget {
        if self.context == 0 {
            return ContextBudget::UNBOUNDED;
        }
        ContextBudget {
            capacity: self.context,
            compaction: Compaction::Evict,
            fidelity: self.fidelity,
            rot: self.rot,
        }
    }
}

pub(crate) enum Mode {
    /// Compare every arm.
    Compare,
    /// Print one episode turn by turn.
    Trace,
    /// Search the policy grid.
    Sweep,
    /// Sweep the context-window model instead: who is still right when the
    /// window is tight.
    ContextSweep,
    /// Sweep the horizon: what a task with a history costs, and who pays it.
    StageSweep,
    /// Sweep the width: what a task with several facets at once costs, and
    /// whether splitting them across seats beats holding them all.
    FacetSweep,
    /// Sweep room size against channel topology: at what size does the way
    /// members reach each other start to matter, and which way.
    ScaleSweep,
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
    pub(crate) fn defaults() -> Self {
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
            sizes: crate::scale::DEFAULT_SIZES.to_vec(),
            horizons: Vec::new(),
            facets: Vec::new(),
            roles: false,
            fidelity: FOLD_FIDELITY,
            cost: false,
            blind_evidence: false,
            defer_cap: 1,
            aside_cap: 1,
            // A round of four is the width `EpisodePolicy::DEFAULT` runs at.
            round_width: tinyhivemind_hive::DEFAULT_ROUND_WIDTH,
            context: 0,
            rot: 0.0,
            exchange_cap: 4,
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
    pub(crate) fn parse() -> Self {
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
            // The ceiling is a spending bound rather than a property of the
            // library, which has no room-size limit.
            options.agents = usize::try_from(agents)
                .unwrap_or(5)
                .clamp(2, crate::sim::MAX_MEMBERS);
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

/// Apply one of `--specialists`, `--hidden-profile`, `--defer-cap`,
/// `--cost-tiers`, `--blind-evidence` or `--directory` to `options`, or do
/// nothing for a flag it does not recognise.
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

#[cfg(test)]
mod test {
    use super::*;

    /// `f64::parse` accepts `"nan"`, `"inf"` and `"-inf"`, and `f64::clamp`
    /// leaves a `NaN` unchanged rather than bounding it -- so an unguarded
    /// `--fidelity nan` would have stored a `NaN` in `options.fidelity`, and
    /// every downstream read of it (`ContextBudget::worth`, in turn
    /// `windowed_score`'s fallback to the agent's own score) would have
    /// silently discarded folded context instead of erroring or defaulting.
    #[test]
    fn a_non_finite_fidelity_is_rejected_rather_than_stored() {
        let mut options = Options::defaults();
        let before = options.fidelity;

        for raw in ["nan", "inf", "-inf"] {
            let mut args = [raw.to_owned()].into_iter();
            apply_expertise_flag(&mut options, "--fidelity", &mut args);
            assert_eq!(
                options.fidelity, before,
                "a non-finite --fidelity value of {raw:?} must be rejected, \
                 leaving the prior value in place",
            );
        }
    }

    #[test]
    fn a_finite_fidelity_is_still_parsed_and_clamped() {
        let mut options = Options::defaults();

        let mut args = ["1.5".to_owned()].into_iter();
        apply_expertise_flag(&mut options, "--fidelity", &mut args);
        assert_eq!(options.fidelity, 1.0, "a finite value is still clamped");

        let mut args = ["0.4".to_owned()].into_iter();
        apply_expertise_flag(&mut options, "--fidelity", &mut args);
        assert_eq!(options.fidelity, 0.4);
    }
}
