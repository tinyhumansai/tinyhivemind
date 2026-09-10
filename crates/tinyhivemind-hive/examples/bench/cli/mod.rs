//! Command-line parsing for the bench binary.
//!
//! Split out of `main.rs` because a CLI surface this wide — the room and
//! federation knobs, both live backends, the delegation and check-arm caps —
//! is a module in its own right, not a line-count problem. [`Options`] is the
//! parsed argument set every other module reads; [`Mode`] is what `--sweep`,
//! `--trace`, `--swarm` and friends pick between. See `main.rs`'s crate doc
//! for the flag table itself.

mod flags;

use crate::context::{Compaction, ContextBudget, FOLD_FIDELITY};
use crate::cost::CostModel;
use crate::grid::Axes;
use crate::http::{Thinking, Wire};
use crate::policy::tuned_policy;
use crate::sim::Expertise;
use flags::{
    apply_expertise_flag, apply_live_flag, apply_mode_flag, apply_scale_flag, flag_number,
    next_number, set_live_floor,
};
use tinyhivemind_hive::EpisodePolicy;

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
    /// The cross product of axes a `--grid` run walks.
    ///
    /// Defaults to a single point reproducing the single-room comparison, so a
    /// bare `--grid` prints one small table and each axis is widened by naming
    /// it. See [`crate::grid`].
    pub(crate) axes: Axes,
    /// What a turn costs in tokens and how long a host waits for one.
    ///
    /// Every table's headline columns are computed against this, and every
    /// run prints it, so a reader sees the point the numbers were taken at
    /// rather than inheriting it. See [`crate::cost`].
    pub(crate) cost_model: CostModel,
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
    /// A directory of them, run in name order, if one was given.
    ///
    /// Takes precedence over `scenario`: a caller that names both meant the
    /// corpus, and running one file out of it instead would be the quieter and
    /// worse reading.
    pub(crate) scenario_dir: Option<String>,
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
    /// Questions one desk may put to other channels, off the floor.
    ///
    /// `0` puts asking back on the floor, where a member spends the turn the
    /// episode authorized on it and every peer may be asked once — the
    /// behaviour every recorded swarm number was taken against, and the one
    /// that collapses above roughly eight desks. Above `0` a desk asks without
    /// taking a turn, at most this many times, however many peers it has.
    pub(crate) ask_cap: usize,
    /// Readings one desk publishes to **every** other channel, off the floor.
    ///
    /// A different mechanism from `ask_cap`, not more of it. An ask is a
    /// question to one peer and costs a question and an answer; a digest is
    /// this desk's reading of the whole slate written once and appended to
    /// every peer's transcript, so it costs one model call however large the
    /// federation is. It exists because a bounded ask cannot recover an error
    /// every desk shares — the peer you would have asked has it too.
    ///
    /// `0` is off, and off is what every recorded number was taken against.
    pub(crate) digest: usize,
    /// Whether the federation's disqualifying **facts** are planted, and so
    /// exist to be exchanged at all.
    ///
    /// Without it a federation holds only scored readings: every member has a
    /// noisy opinion of every option and a slant toward its own desk's decoy,
    /// so the only thing a channel can carry is an opinion — and averaging
    /// opinions imports a shared bias rather than cancelling it. With it, one
    /// member per desk holds a fact that disqualifies **another** desk's
    /// decoy, so the cure for a desk's blind spot is real, held, and
    /// elsewhere.
    ///
    /// It changes the task rather than only the wire, so every arm is
    /// re-baselined under it and nothing recorded without it moves.
    pub(crate) evidence: bool,
    /// Per mille of planted facts that name the **truth** rather than the
    /// decoy they were meant to disqualify.
    ///
    /// The adversarial half of `--evidence`. A fact does not average, which is
    /// why it survives a shared bias and why a wrong one is worse than a wrong
    /// opinion: it subtracts a flat discount from the right answer for every
    /// desk it reaches. `0` keeps every planted fact true, which measures the
    /// value of the channel and nothing about the risk of it.
    pub(crate) fact_noise: u32,
    /// Threads the per-room loops are spread across.
    ///
    /// A wall-clock knob and nothing else: rooms are independent and results
    /// are folded in room order whatever order the threads finish in, so the
    /// same `--seed` prints the same bytes at every value. See
    /// [`crate::parallel`].
    pub(crate) jobs: usize,
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
    /// Walk the cross product of topic, scale, complexity and concurrency,
    /// reporting the same six columns in every cell.
    Grid,
    /// Measure the cost model's constants against a live endpoint and print
    /// the flags that reproduce them.
    Calibrate,
}

impl Options {
    /// The options every mode starts from before a flag overrides one.
    ///
    /// `pub(crate)` so a sweep's own room constructor can be tested against a
    /// known starting point. [`Self::parse`] reads the process arguments and
    /// is therefore useless to a test.
    pub(crate) fn defaults() -> Self {
        Self {
            axes: Axes::point(),
            cost_model: CostModel::DEFAULT,
            episodes: 500,
            agents: 5,
            topics: 4,
            noise: 90,
            seed: 1,
            policy: tuned_policy(5),
            mode: Mode::Compare,
            scenario: None,
            scenario_dir: None,
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
            // Two, because one outside reading is already enough to overturn
            // a desk's decoy at the default bias — see `SWARM_BIAS` above —
            // and a second covers the case where the first peer asked shares
            // the blind spot. It is a default, not a finding: `--ask-cap`
            // exists because the right width depends on what a question costs
            // the host.
            ask_cap: 2,
            // Off. It is a new mechanism and the convention here is that a
            // new mechanism ships off until an arm has scored it.
            digest: 0,
            evidence: false,
            fact_noise: 0,
            jobs: crate::parallel::default_jobs(),
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
    ///
    /// # Errors
    ///
    /// Returns the flags this harness does not recognise, rather than running
    /// something other than what was asked for.
    pub(crate) fn parse() -> Result<Self, String> {
        let mut options = Self::defaults();
        let mut unknown: Vec<String> = Vec::new();
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
                "--agent-cmd" => {
                    if let Some(command) = args.next() {
                        options.agent = Some(command);
                        // `--swarm --agent-cmd` drives a federation rather
                        // than one room, so the swarm mode keeps the floor;
                        // `set_live_floor` only promotes the parser's own
                        // default, so every other explicit mode does too.
                        set_live_floor(&mut options);
                    }
                }
                "--scenario" => options.scenario = args.next(),
                "--repeat" => options.repeat = next_number(&mut args).unwrap_or(1).max(1),
                "--json" => options.json = true,
                // Everything below is either the expertise surface or the
                // live-backend one: a CLI or HTTP seat, per-seat overrides,
                // and the usage table. Split into their own functions so
                // `parse` itself stays under the line budget clippy holds
                // every function to.
                _ => {
                    // Naming an axis selects the grid, so `--topic hidden`
                    // alone does what it plainly says rather than being
                    // silently ignored by whichever mode happened to be
                    // selected -- the same reason an unrecognised flag is
                    // refused below. But it must not *overwrite* a mode the
                    // operator already chose explicitly: `--swarm --topic
                    // hidden` runs a federation over the named topic, not a
                    // grid, regardless of which flag came first. Only the
                    // parser's own default is safe to promote.
                    if options.axes.set(&flag, &mut args)? {
                        select_grid_axis(&mut options);
                        continue;
                    }
                    let known = apply_mode_flag(&mut options, &flag)
                        || options.cost_model.set(&flag, &mut args)?
                        || apply_scale_flag(&mut options, &flag, &mut args)?
                        || apply_expertise_flag(&mut options, &flag, &mut args)
                        || apply_live_flag(&mut options, &flag, &mut args);
                    // An unrecognised flag used to be discarded in silence, so
                    // `--scale-sweeps` ran the default comparison and reported
                    // it under the heading the operator thought they had asked
                    // for. A benchmark that quietly measures something other
                    // than what was requested is worse than one that refuses.
                    if !known && flag.starts_with("--") {
                        unknown.push(flag);
                    }
                }
            }
        }
        if !unknown.is_empty() {
            return Err(format!("unrecognised flag(s): {}", unknown.join(", ")));
        }
        Ok(options)
    }
}

/// Promote the parser's default mode to [`Mode::Grid`] when an axis flag is
/// given, without overwriting a mode the operator already chose explicitly.
///
/// Naming an axis (`--topic`, `--scale`, `--complexity`, `--concurrency`)
/// alone selects the grid, so it is never silently ignored -- but
/// `--swarm --topic hidden` must keep running the federation `--swarm` asked
/// for rather than switching to a grid because the axis flag happened to
/// come second. Only [`Mode::Compare`], the parser's own default, is safe to
/// promote.
fn select_grid_axis(options: &mut Options) {
    if matches!(options.mode, Mode::Compare) {
        options.mode = Mode::Grid;
    }
}

#[cfg(test)]
mod test;
