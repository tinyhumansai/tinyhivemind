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
//!
//! `rows/ep` is reported beside accuracy because it is the third axis of scale
//! and the one a host pays for: a channel that buys two points by writing four
//! times the transcript has not obviously bought anything.

use std::time::Instant;

use tinyhivemind_hive::episode::EpisodePolicy;

use crate::TASK;
use crate::arms;
use crate::cli::Options;
use crate::metrics::Aggregate;
use crate::policy::tuned_policy;
use crate::run::{
    AsideMode, CheckStyle, run_episode, run_episode_checking, run_episode_exchanging_with,
};
use crate::rng::mix;
use crate::sim::{MAX_MEMBERS, Room};

/// The room sizes swept when `--sizes` is not given.
///
/// Doubling rather than stepping: the interesting behaviour is a curve over
/// orders of magnitude, and a linear ladder spends most of its time where
/// nothing changes.
pub(crate) const DEFAULT_SIZES: [usize; 6] = [3, 5, 8, 16, 32, 64];

/// One arm's score at one room size.
struct Point {
    size: usize,
    arm: &'static str,
    correct: f64,
    turns: f64,
    rows: f64,
}

/// Sweep every size, and every channel at every size.
///
/// # Errors
///
/// Returns whatever an episode returns when the host contract is violated.
pub(crate) fn sweep(options: &Options) -> Result<(), String> {
    let wall = Instant::now();
    let sizes: Vec<usize> = options
        .sizes
        .iter()
        .map(|size| (*size).clamp(2, MAX_MEMBERS))
        .collect();
    let mut points: Vec<Point> = Vec::new();

    for size in &sizes {
        let tuned = tuned_policy(*size);
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
    let mut ladder = Aggregate::default();
    let mut vote = Aggregate::default();
    let mut broadcast = Aggregate::default();
    let mut on_floor = Aggregate::default();
    let mut rounds = Aggregate::default();
    let mut off_floor = Aggregate::default();
    let mut pooled = Aggregate::default();

    for room in rooms {
        ladder.add_arm(&arms::run_ladder(room, options.seed)?);
        vote.add_arm(&arms::run_vote(room, tuned.turn_budget));
        broadcast.add(&run_episode(room, tuned, TASK, false)?);
        on_floor.add(&run_episode_checking(
            room,
            tuned,
            TASK,
            false,
            0,
            AsideMode::Private,
            options.aside_cap,
            CheckStyle::FACT,
        )?);
        rounds.add(&run_episode_exchanging_with(
            room,
            tuned,
            TASK,
            false,
            options.exchange_cap,
            CheckStyle::EXCHANGE,
        )?);
        off_floor.add(&run_episode(
            &room.pre_checked(options.aside_cap, true),
            tuned,
            TASK,
            false,
        )?);
        pooled.add(&run_episode(&room.pooled(), tuned, TASK, false)?);
    }

    let named = [
        ("ladder", ladder),
        ("vote", vote),
        ("hive+", broadcast),
        ("hive+fact", on_floor),
        ("hive+rounds", rounds),
        ("hive+fact°", off_floor),
        ("hive+pooled", pooled),
    ];
    Ok(named
        .into_iter()
        .map(|(arm, totals)| Point {
            size,
            arm,
            correct: totals.accuracy(),
            turns: totals.turns_per_episode(),
            rows: totals.rows_per_episode(),
        })
        .collect())
}

/// One table per column of interest, sizes across the top.
fn render(points: &[Point], sizes: &[usize], options: &Options) -> String {
    let mut out = String::new();
    let task = if options.expertise_is_hidden_profile() {
        "hidden profile"
    } else {
        "uniform noise"
    };
    out.push_str(&format!(
        "task {task}  rooms {} per size  options {}  eval noise ±{}\n\n",
        options.episodes, options.topics, options.noise,
    ));

    for (title, field) in [
        ("correct %", 0_usize),
        ("turns/ep", 1),
        ("rows/ep", 2),
    ] {
        out.push_str(&format!("{title}\n\narm         "));
        for size in sizes {
            out.push_str(&format!("{size:>9}");
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
        ] {
            out.push_str(&format!("{arm:<12}"));
            for size in sizes {
                let cell = points
                    .iter()
                    .find(|point| point.arm == arm && point.size == *size);
                match cell {
                    Some(point) => {
                        let value = match field {
                            0 => point.correct,
                            1 => point.turns,
                            _ => point.rows,
                        };
                        out.push_str(&format!("{value:>9.1}"));
                    }
                    None => out.push_str(&format!("{:>9}", "—")),
                }
            }
            out.push('\n');
        }
        out.push('\n');
    }
    out
}
