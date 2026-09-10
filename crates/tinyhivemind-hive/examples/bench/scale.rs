//! The size-by-topology sweep: what a room's channel is worth as the room grows.
//!
//! The arm comparison next door answers *which protocol* on a room of five.
//! This answers a different question, and one the five-member table cannot see:
//! **at what room size does the channel start to matter, and which channel.**
//!
//! # Why the question is not already answered
//!
//! The recorded size table shows `hive+`'s margin over the matched-budget vote
//! decaying with the room — `+3.2` at three members, `+1.2` at eight. Extend
//! the ladder past eight and the decay does not level off, it *crosses*: on the
//! uniform task a plurality of independent noisy readings converges on the
//! truth all by itself, so by sixteen members there is nothing left for any
//! protocol to add and both arms saturate near 100%. A benchmark run there
//! measures the law of large numbers, not the library.
//!
//! The regime worth sweeping is the one where pooling does *not* trivially
//! win — `--hidden-profile`, where a decoy sits above every member's own
//! argmax except one, and that one member holds the fact that rules it out.
//! There, growing the room makes things monotonically **worse** for every
//! majority mechanism, because the lone fact-holder is one voice among more and
//! more. That is exactly the regime where reaching the *right* peer should beat
//! addressing everybody, and it is what this sweep is for.
//!
//! # The topology ladder
//!
//! Each row is the same rooms decided by a different channel, ordered by how
//! much the channel costs the floor:
//!
//! | arm | channel |
//! | --- | --- |
//! | `ladder` | nobody talks; one responder answers alone |
//! | `vote` | nobody talks; every member answers and a plurality decides |
//! | `hive+` | broadcast: every row reaches the whole room |
//! | `hive+fact` | broadcast, plus one pairwise check that **spends a turn** |
//! | `hive+rounds` | broadcast, plus continuous pairwise **off the floor** |
//! | `hive+fact°` | broadcast, plus one bounded pairwise check off the floor |
//! | `hive+pooled` | the ceiling: everything every member holds, free |
//! | `hive+wide` | broadcast, every round widened — concurrency in full |
//! | `hive+blind` | broadcast, widened only while blind — the free half |
//!
//! `rounds/ep` sits beside `turns/ep` because depth and width are different
//! costs and the ladder is where they diverge most: at sixty-four members
//! `hive+` is 65.1 rounds deep and `hive+blind` is 31.0, for the same accuracy
//! to a tenth of a point.
//!
//! `rows/ep` is reported beside accuracy because it is the third axis of scale
//! and the one a host pays for: a channel that buys two points by writing four
//! times the transcript has not obviously bought anything.

use std::fmt::Write as _;
use std::time::Instant;

use tinyhivemind_hive::episode::EpisodePolicy;

use crate::TASK;
use crate::arms;
use crate::cli::Options;
use crate::metrics::Aggregate;
use crate::parallel;
use crate::policy::{blind_wide_policy, tuned_policy, widened_policy};
use crate::rng::mix;
use crate::run::{
    AsideMode, CheckStyle, run_episode, run_episode_checking, run_episode_exchanging_with,
};
use crate::sim::{Expertise, MAX_MEMBERS, Room};

/// The room sizes swept when `--sizes` is not given.
///
/// Doubling rather than stepping: the interesting behaviour is a curve over
/// orders of magnitude, and a linear ladder spends most of its time where
/// nothing changes.
pub(crate) const DEFAULT_SIZES: [usize; 6] = [3, 5, 8, 16, 32, 64];

/// What every channel decided about one room.
///
/// One room's worth of work, so a worker can produce it without touching any
/// other room and the parent can fold the lot in room order. Every field is
/// the report of the arm named after it in the table.
struct RoomOutcome {
    ladder: crate::arms::ArmReport,
    vote: crate::arms::ArmReport,
    broadcast: crate::run::EpisodeReport,
    on_floor: crate::run::EpisodeReport,
    rounds: crate::run::EpisodeReport,
    off_floor: crate::run::EpisodeReport,
    pooled: crate::run::EpisodeReport,
    wide: crate::run::EpisodeReport,
    blind_wide: crate::run::EpisodeReport,
}

/// One arm's score at one room size.
struct Point {
    size: usize,
    arm: &'static str,
    correct: f64,
    turns: f64,
    /// Rounds per episode: the arm's depth, what a host with async seats waits
    /// for. The whole reason this sweep needed a second cost column.
    depth: f64,
    rows: f64,
}

/// One channel's running totals over one size's rooms.
///
/// `Aggregate` carries everything except the private-message count, which is
/// the third axis this sweep exists to show — floor rows are `turns/ep`, and
/// what a channel writes *beside* the floor is this. Accumulated here rather
/// than pushed into the shared metric, which no other mode needs it in.
#[derive(Default)]
struct Channel {
    totals: Aggregate,
    rows: f64,
    episodes: u32,
}

impl Channel {
    fn add(&mut self, report: &crate::run::EpisodeReport) {
        self.rows += f64::from(report.contacts);
        self.episodes = self.episodes.saturating_add(1);
        self.totals.add(report);
    }

    /// A control arm holds no private channel at all, so it contacts nobody.
    fn add_arm(&mut self, report: &crate::arms::ArmReport) {
        let _ = report;
        self.episodes = self.episodes.saturating_add(1);
        self.totals.add_arm(report);
    }

    fn rows_per_episode(&self) -> f64 {
        if self.episodes == 0 {
            return 0.0;
        }
        self.rows / f64::from(self.episodes)
    }
}

/// Sweep every size, and every channel at every size.
///
/// # Errors
///
/// Returns whatever an episode returns when the host contract is violated.
pub(crate) fn sweep(options: &Options) -> Result<(), String> {
    let wall = Instant::now();
    // Clamped, then deduped, and in that order. Two requested sizes above the
    // cap clamp to the same number, and a `sizes` list carrying it twice would
    // run every arm twice and push two `Point`s per arm into the table, where
    // `render`'s `find` silently returns the first — a column labelled with a
    // size the harness never ran. Saying so beats swallowing it.
    let mut sizes: Vec<usize> = Vec::new();
    for requested in &options.sizes {
        let size = (*requested).clamp(2, MAX_MEMBERS);
        if size != *requested {
            println!(
                "note: room size {requested} clamped to {size}, the largest this harness builds"
            );
        }
        if !sizes.contains(&size) {
            sizes.push(size);
        }
    }
    let mut points: Vec<Point> = Vec::new();

    for size in &sizes {
        // The size-scaled policy, carrying whatever `--distance` asked for:
        // `tuned_policy` knows the room size and nothing about the ruler.
        let tuned = EpisodePolicy {
            distance: options.policy.distance,
            ..tuned_policy(*size)
        };
        let rooms: Vec<Room> = (0..options.episodes)
            .map(|index| {
                let mut room = Room::generate_with(
                    mix(options.seed, u64::from(index)),
                    *size,
                    options.topics,
                    options.noise,
                    options.expertise,
                    options.cost,
                );
                room.set_blind_evidence(options.blind_evidence);
                room
            })
            .collect();
        points.extend(one_size(options, &rooms, *size, &tuned)?);
    }

    print!("{}", render(&points, &sizes, options));
    println!("\nswept in {:.2} s", wall.elapsed().as_secs_f64());
    Ok(())
}

/// Every channel, over one size's rooms.
fn one_size(
    options: &Options,
    rooms: &[Room],
    size: usize,
    tuned: &EpisodePolicy,
) -> Result<Vec<Point>, String> {
    // Every room's seven arms are computed in a worker and the reports are
    // folded here, in room order. The fold has to stay ordered even though the
    // work does not: `Aggregate::correct_flags` is positional and the paired
    // bootstrap under the table resamples arms index-for-index because they
    // decided the *same* rooms. See `crate::parallel`.
    let decided: Vec<RoomOutcome> = parallel::map_in_order(rooms, options.jobs, |room| {
        Ok(RoomOutcome {
            ladder: arms::run_ladder(room, options.seed)?,
            vote: arms::run_vote(room, tuned.turn_budget),
            broadcast: run_episode(room, tuned, TASK, false)?,
            on_floor: run_episode_checking(
                room,
                tuned,
                TASK,
                false,
                0,
                AsideMode::Private,
                options.aside_cap,
                CheckStyle::FACT,
            )?,
            rounds: run_episode_exchanging_with(
                room,
                tuned,
                TASK,
                false,
                options.exchange_cap,
                CheckStyle::EXCHANGE,
            )?,
            off_floor: run_episode(
                &room.pre_checked(options.aside_cap, true),
                tuned,
                TASK,
                false,
            )?,
            // `--aside-cap 0` is documented and used as the kill switch that
            // leaves every aside arm bit-identical to `hive+`, `hive+pooled`
            // included (see `compare.rs`), so honor it here too by skipping the
            // pool rather than silently pooling regardless of the cap.
            pooled: run_episode(
                &if options.aside_cap == 0 {
                    room.clone()
                } else {
                    room.pooled()
                },
                tuned,
                TASK,
                false,
            )?,
            // The same broadcast room, run in concurrent rounds. This is the
            // arm the scale result asks for: every floor-bound channel dies as
            // the room grows because turns per member is `budget / n`, and a
            // round is the only thing that changes that ratio without changing
            // the budget.
            wide: run_episode(
                room,
                &widened_policy(tuned, options.round_width),
                TASK,
                false,
            )?,
            blind_wide: run_episode(
                room,
                &blind_wide_policy(tuned, options.round_width),
                TASK,
                false,
            )?,
        })
    })?;

    let mut ladder = Channel::default();
    let mut vote = Channel::default();
    let mut broadcast = Channel::default();
    let mut on_floor = Channel::default();
    let mut rounds = Channel::default();
    let mut off_floor = Channel::default();
    let mut pooled = Channel::default();
    let mut wide = Channel::default();
    let mut blind_wide = Channel::default();
    for outcome in &decided {
        ladder.add_arm(&outcome.ladder);
        vote.add_arm(&outcome.vote);
        broadcast.add(&outcome.broadcast);
        on_floor.add(&outcome.on_floor);
        rounds.add(&outcome.rounds);
        off_floor.add(&outcome.off_floor);
        pooled.add(&outcome.pooled);
        wide.add(&outcome.wide);
        blind_wide.add(&outcome.blind_wide);
    }

    let named = [
        ("ladder", ladder),
        ("vote", vote),
        ("hive+", broadcast),
        ("hive+fact", on_floor),
        ("hive+rounds", rounds),
        ("hive+fact°", off_floor),
        ("hive+pooled", pooled),
        ("hive+wide", wide),
        ("hive+blind", blind_wide),
    ];
    Ok(named
        .into_iter()
        .map(|(arm, channel)| Point {
            size,
            arm,
            correct: channel.totals.accuracy(),
            turns: channel.totals.turns_per_episode(),
            depth: channel.totals.rounds_per_episode(),
            rows: channel.rows_per_episode(),
        })
        .collect())
}

/// One table per column of interest, sizes across the top.
fn render(points: &[Point], sizes: &[usize], options: &Options) -> String {
    let mut out = String::new();
    let task = match options.expertise {
        Expertise::HiddenProfile => "hidden profile",
        Expertise::Specialists { .. } => "specialists",
        Expertise::Roles { .. } => "roles",
        Expertise::Uniform => "uniform noise",
    };
    let _ = write!(
        out,
        "task {task}  rooms {} per size  options {}  eval noise ±{}\n\n",
        options.episodes, options.topics, options.noise,
    );

    for (title, field) in [
        ("correct %", 0_usize),
        ("turns/ep — rows on the floor", 1),
        ("rounds/ep — depth, what a host waits for", 3),
        ("contacts/ep — members asked privately", 2),
    ] {
        let _ = write!(out, "{title}\n\narm         ");
        for size in sizes {
            let _ = write!(out, "{size:>9}");
        }
        out.push('\n');
        for arm in [
            "ladder",
            "vote",
            "hive+",
            "hive+fact",
            "hive+rounds",
            "hive+fact°",
            "hive+pooled",
            "hive+wide",
            "hive+blind",
        ] {
            let _ = write!(out, "{arm:<12}");
            for size in sizes {
                let cell = points
                    .iter()
                    .find(|point| point.arm == arm && point.size == *size);
                match cell {
                    Some(point) => {
                        let value = match field {
                            0 => point.correct,
                            1 => point.turns,
                            3 => point.depth,
                            _ => point.rows,
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
