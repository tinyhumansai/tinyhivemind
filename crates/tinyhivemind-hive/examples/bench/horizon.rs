//! The long-horizon sweep: what a task with a history costs, and who pays it.
//!
//! Every other mode here measures **one** decision. This one measures a chain
//! of them, because the claim the library exists to test is about the shape of
//! the work rather than the size of the room:
//!
//! > A single agent with context compaction wins at small scale and short
//! > horizon. A room of agents should win as the work gets longer.
//!
//! Neither half of that is stateable against a single categorical choice.
//! Nothing compounds — a room that answers wrong pays once, and there is no
//! stage two for a wrong stage one to poison. Nothing accumulates — the window
//! model is real, but an episode tops out at about seventeen rows, so a window
//! never binds the way it binds in practice. And the control the claim is
//! actually about is not an arm: `ladder` is one turn, `vote` is `n`
//! independent one-shots, and neither is *one agent working a long task*.
//!
//! # The chain
//!
//! `--stages N` runs N sub-decisions in sequence, decided by the same members.
//! Three things make it a horizon rather than a repeat:
//!
//! - **Stage `k`'s options are named distinctly**, so a reading taken at one
//!   stage can never be scored against another's question. It can only take up
//!   a row.
//! - **Context accumulates.** What a member was told at stage `k` is still in
//!   its window at stage `k+1`, relevant or not.
//! - **A wrong stage poisons the next.** The room is then reasoning from a
//!   false premise, and the option consistent with it reads better than it is.
//!
//! # The arms
//!
//! | arm | what it is |
//! | --- | --- |
//! | `solo` | one agent handed every member's readings and facts, deciding each stage alone. The whole brief, one window, compacting by eviction |
//! | `solo+fold` | the same agent, compacting by a superseding account instead: what leaves the window is summarised, not dropped |
//! | `hive+` | the room, deliberating each stage. Each member holds only what it was given |
//! | `hive+pooled` | the ceiling: every reading in every member's hands, free |
//!
//! `end-to-end %` is the headline because compounding is the point. `held/ep`
//! is the axis the task exists to stress: a room of `n` splits one brief `n`
//! ways.

use std::fmt::Write as _;
use std::time::Instant;

use tinyhivemind_hive::episode::EpisodePolicy;

use crate::TASK;
use crate::cli::Options;
use crate::context::Compaction;
use crate::policy::tuned_policy;
use crate::rng::mix;
use crate::run::run_episode;
use crate::sim::{POISON_LIFT, Room};

/// One arm's score over a sample of chains.
#[derive(Default)]
struct Chain {
    /// Chains in which every stage was decided correctly.
    whole: u32,
    /// Stages decided correctly, across every chain.
    stages_right: u32,
    /// Stages attempted, across every chain.
    stages: u32,
    /// Chains run.
    chains: u32,
    /// Rows a participant was carrying when the chain ended, summed.
    held: f64,
    /// Rounds spent across every stage of every chain.
    rounds: u64,
    /// Turns spent across every stage of every chain.
    turns: u64,
}

impl Chain {
    fn add(&mut self, run: &ChainRun) {
        self.chains = self.chains.saturating_add(1);
        self.stages = self.stages.saturating_add(run.stages);
        self.stages_right = self.stages_right.saturating_add(run.right);
        if run.right == run.stages && run.stages > 0 {
            self.whole = self.whole.saturating_add(1);
        }
        self.held += run.held;
        self.rounds = self.rounds.saturating_add(u64::from(run.rounds));
        self.turns = self.turns.saturating_add(u64::from(run.turns));
    }

    fn end_to_end(&self) -> f64 {
        ratio(self.whole.into(), self.chains.into()) * 100.0
    }

    fn per_stage(&self) -> f64 {
        ratio(self.stages_right.into(), self.stages.into()) * 100.0
    }

    fn held_per_chain(&self) -> f64 {
        ratio_f64(self.held, self.chains.into())
    }

    fn rounds_per_chain(&self) -> f64 {
        ratio_f64(as_f64(self.rounds), self.chains.into())
    }

    fn turns_per_chain(&self) -> f64 {
        ratio_f64(as_f64(self.turns), self.chains.into())
    }
}

/// What one chain cost one arm.
#[derive(Default)]
struct ChainRun {
    stages: u32,
    right: u32,
    rounds: u32,
    turns: u32,
    held: f64,
}

/// One arm's score at one horizon length.
struct Point {
    stages: usize,
    arm: &'static str,
    end_to_end: f64,
    per_stage: f64,
    held: f64,
    depth: f64,
}

/// The horizons swept when `--stages` names only one length.
///
/// Doubling rather than stepping, for the reason the size ladder doubles: the
/// interesting behaviour is a curve over orders of magnitude, and a linear
/// ladder spends most of its time where nothing changes.
pub(crate) const DEFAULT_HORIZONS: [usize; 5] = [1, 2, 4, 8, 16];

/// Run the chain ladder and print what each arm scored.
///
/// # Errors
///
/// Returns the library's own error text if a policy or snapshot is malformed.
pub(crate) fn sweep(options: &Options) -> Result<(), String> {
    let horizons = if options.horizons.is_empty() {
        DEFAULT_HORIZONS.to_vec()
    } else {
        options.horizons.clone()
    };
    let started = Instant::now();
    let mut points = Vec::new();
    for stages in &horizons {
        points.extend(one_horizon(options, *stages)?);
    }
    print!("{}", render(&points, &horizons, options));
    println!("swept in {:.2} s", started.elapsed().as_secs_f64());
    Ok(())
}

/// Score every arm over one horizon length.
fn one_horizon(options: &Options, stages: usize) -> Result<Vec<Point>, String> {
    let tuned = tuned_policy(options.agents);
    let mut solo = Chain::default();
    let mut folding = Chain::default();
    let mut room_arm = Chain::default();
    let mut pooled = Chain::default();

    for episode in 0..options.episodes {
        let seed = mix(options.seed, u64::from(episode));
        solo.add(&run_alone(options, seed, stages, Compaction::Evict));
        folding.add(&run_alone(options, seed, stages, Compaction::Fold));
        room_arm.add(&run_room(options, seed, stages, &tuned, false)?);
        pooled.add(&run_room(options, seed, stages, &tuned, true)?);
    }

    Ok([
        ("solo", solo),
        ("solo+fold", folding),
        ("hive+", room_arm),
        ("hive+pooled", pooled),
    ]
    .into_iter()
    .map(|(arm, chain)| Point {
        stages,
        arm,
        end_to_end: chain.end_to_end(),
        per_stage: chain.per_stage(),
        held: chain.held_per_chain(),
        depth: chain.rounds_per_chain(),
    })
    .collect())
}

/// Build the room one stage of a chain runs on.
///
/// Renamed for this stage, inheriting what the last one left in every member's
/// window, and poisoned if the room got the last one wrong.
fn staged(options: &Options, seed: u64, stage: usize, prior: Option<&Room>, wrong: bool) -> Room {
    let base = Room::generate_with(
        mix(seed, 0x5741_4745 ^ stage as u64),
        options.agents,
        options.topics,
        options.noise,
        options.expertise,
        options.cost,
    );
    let mut room = base.for_stage(stage);
    if let Some(prior) = prior {
        room = room.inheriting(prior);
    }
    if wrong {
        room = room.poisoned(POISON_LIFT);
    }
    // Every member pays for the brief it was handed, before any arm decides
    // what to do with it. A soloist that is later given every peer's brief
    // pays for those too, on top.
    room.charge_brief();
    room.set_budget(options.budget());
    room
}

/// One chain, decided by a single agent holding the whole brief.
///
/// The soloist is the room's own first member with every peer's readings and
/// every peer's facts already installed — which is what "handed the whole
/// brief" means, stated in the same units the room is stated in. It answers
/// from its own argmax: one turn, one round, no deliberation to have.
fn run_alone(options: &Options, seed: u64, stages: usize, compaction: Compaction) -> ChainRun {
    let mut run = ChainRun::default();
    let mut prior: Option<Room> = None;
    let mut wrong = false;
    for stage in 0..stages {
        let mut room = staged(options, seed, stage, prior.as_ref(), wrong);
        // Everything the room collectively holds, in one member's window.
        room = room.pooled();
        room.set_compaction(compaction);
        let Some(agent) = room.agents.first() else {
            break;
        };
        let correct = *agent.favourite() == room.truth;
        run.stages = run.stages.saturating_add(1);
        run.turns = run.turns.saturating_add(1);
        run.rounds = run.rounds.saturating_add(1);
        if correct {
            run.right = run.right.saturating_add(1);
        }
        wrong = !correct;
        run.held = room.agents.first().map_or(0.0, |held| held.held() as f64);
        prior = Some(room);
    }
    run
}

/// One chain, decided by the room.
fn run_room(
    options: &Options,
    seed: u64,
    stages: usize,
    policy: &EpisodePolicy,
    pool: bool,
) -> Result<ChainRun, String> {
    let mut run = ChainRun::default();
    let mut prior: Option<Room> = None;
    let mut wrong = false;
    for stage in 0..stages {
        let mut room = staged(options, seed, stage, prior.as_ref(), wrong);
        if pool {
            room = room.pooled();
        }
        let report = run_episode(&room, policy, TASK, false)?;
        run.stages = run.stages.saturating_add(1);
        run.turns = run.turns.saturating_add(report.turns);
        run.rounds = run.rounds.saturating_add(report.rounds);
        if report.correct {
            run.right = run.right.saturating_add(1);
        }
        wrong = !report.correct;
        run.held = room.held();
        prior = Some(room);
    }
    Ok(run)
}

/// One table per column of interest, horizons across the top.
fn render(points: &[Point], horizons: &[usize], options: &Options) -> String {
    let mut out = String::new();
    let window = if options.context == 0 {
        "unbounded".to_owned()
    } else {
        format!("{} rows, rot {:.1}", options.context, options.rot)
    };
    let _ = write!(
        out,
        "chains {} per horizon  agents {}  options {}  window {window}\n\n",
        options.episodes, options.agents, options.topics,
    );

    for (title, field) in [
        ("end-to-end % — every stage right", 0_usize),
        ("stage % — per-decision", 1),
        ("held/ep — rows one participant carries", 2),
        ("rounds/ep — depth over the whole chain", 3),
    ] {
        let _ = write!(out, "{title}\n\narm          ");
        for stages in horizons {
            let _ = write!(out, "{stages:>9}");
        }
        out.push('\n');
        for arm in ["solo", "solo+fold", "hive+", "hive+pooled"] {
            let _ = write!(out, "{arm:<13}");
            for stages in horizons {
                let cell = points
                    .iter()
                    .find(|point| point.arm == arm && point.stages == *stages);
                match cell {
                    Some(point) => {
                        let value = match field {
                            0 => point.end_to_end,
                            1 => point.per_stage,
                            2 => point.held,
                            _ => point.depth,
                        };
                        let _ = write!(out, "{value:>9.1}");
                    }
                    None => {
                        let _ = write!(out, "{:>9}", "—");
                    }
                }
            }
            out.push('\n');
        }
        out.push('\n');
    }
    out
}

/// Integer ratio as a float, zero for an empty denominator.
fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    as_f64(numerator) / as_f64(denominator)
}

/// The same for a running float total.
fn ratio_f64(numerator: f64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    numerator / as_f64(denominator)
}

/// Widen a count for the arithmetic above.
#[expect(
    clippy::cast_precision_loss,
    reason = "a sample larger than an f64 can count is not a sample this harness runs"
)]
fn as_f64(value: u64) -> f64 {
    value as f64
}
