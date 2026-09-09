//! Who is still right when the window is tight.
//!
//! The rest of the benchmark asks which protocol decides best when information
//! transfer is free and lossless. That is a fair question and `hive+pooled`
//! answers it: hand every member every peer's reading and the room is far
//! better than any protocol that has to *move* the information.
//!
//! It is also the question a deployed system never gets to ask. A participant
//! is a language model with a window, "hand it everything" means putting
//! everything in that window, and
//! [`../../../../docs/research/long-context.md`](../../../../docs/research/long-context.md)
//! is unkind about what that does: retrieval is worst in the middle of the
//! input, and quality degrades well before the window is full. `pooled` is not
//! a ceiling anybody can stand on — it is what the room would decide if
//! telepathy were free.
//!
//! This sweep charges for it. Each arm runs across a ladder of window
//! capacities, so the reported quantity is not "who is best" but **"who
//! degrades first"** — and the arms differ precisely in how many rows they need
//! a member to hold in order to work.
//!
//! # Read the ordering, not the numbers
//!
//! [`crate::context`] is a model of two published effects, not a measurement of
//! any real model's retrieval. A single row of this table means very little.
//! What means something is whether an ordering *survives the whole sweep*: if
//! one arm leads at every capacity and every `--rot`, that is a claim about the
//! protocols. If the lead changes hands when `--rot` moves, that is a claim
//! about [`crate::context`], and the honest report is that the benchmark cannot
//! separate them.
//!
//! The `rot` column is swept for exactly that reason. It is easy to build a
//! context model that proves whatever the author already believed; the defence
//! is to show the belief holding across the parameter's whole plausible range,
//! or to say plainly that it does not.

use std::fmt::Write as _;

use tinyhivemind_hive::EpisodePolicy;

use crate::context::ContextBudget;
use crate::metrics::Aggregate;
use crate::run::{AsideMode, CheckStyle, EpisodeReport, run_episode, run_episode_checking};
use crate::sim::Room;

/// Window capacities the sweep walks, in rows.
///
/// `0` is the unbounded control and must come first: it reproduces the
/// benchmark's existing numbers, so a reader can see that the model changed
/// nothing before it was switched on.
const CAPACITIES: [usize; 6] = [0, 4, 8, 16, 32, 64];

/// How hard the middle of the window is discounted.
///
/// `0.0` charges only for eviction; `1.0` is the strongest U this model
/// expresses. The truth for any given model is somewhere inside, and the point
/// of sweeping is that the reader gets to see the whole interval rather than
/// the author's favourite point in it.
const ROTS: [f64; 3] = [0.0, 0.5, 1.0];

/// One arm's result at one window setting.
pub(crate) struct Point {
    arm: &'static str,
    capacity: usize,
    rot: f64,
    correct: f64,
    decided: f64,
    /// Mean rows a member was holding when the episode ended — what the arm
    /// costs the window, as opposed to what it costs the floor.
    rows: f64,
}

/// The arms this sweep compares, and why each is here.
///
/// Deliberately four rather than twenty. The question is about context cost,
/// and these are the four points on that axis: one agent holding almost
/// nothing, a room holding its own transcript, a room whose members also hold
/// compressed stubs of exchanges they were not in, and the arm that hands
/// everybody everything.
fn arms(
    room: &Room,
    policy: &EpisodePolicy,
    task: &str,
    aside_cap: u32,
) -> Result<Vec<(&'static str, EpisodeReport)>, String> {
    Ok(vec![
        // The room, reading its own transcript and nothing else.
        ("hive+", run_episode(room, policy, task, false)?),
        // The same room, with a private exchange riding alongside each floor
        // move. A member outside one holds a single collapsed stub for it.
        (
            "hive+along",
            run_episode_checking(
                room,
                policy,
                task,
                false,
                0,
                AsideMode::Alongside,
                aside_cap,
                CheckStyle::ALONGSIDE,
            )?,
        ),
        // Everybody holds everybody's readings. Free under the old model; here
        // it pays for every row it takes.
        (
            "hive+pooled",
            run_episode(&room.pooled(), policy, task, false)?,
        ),
    ])
}

/// Run the ladder and return every point.
pub(crate) fn sweep(
    rooms: &[Room],
    policy: &EpisodePolicy,
    task: &str,
    aside_cap: u32,
) -> Result<Vec<Point>, String> {
    let mut points = Vec::new();
    for rot in ROTS {
        for capacity in CAPACITIES {
            // A capacity of zero is the unbounded control and `rot` cannot
            // mean anything for it, so it is run once rather than three times
            // reporting the same row.
            if capacity == 0 && rot != ROTS[0] {
                continue;
            }
            let budget = ContextBudget {
                capacity,
                rot,
                ..ContextBudget::UNBOUNDED
            };
            let mut totals: Vec<(&'static str, Aggregate, f64, usize)> = Vec::new();
            for room in rooms {
                let windowed = room.with_budget(budget);
                for (arm, report) in arms(&windowed, policy, task, aside_cap)? {
                    if let Some((_, aggregate, rows, count)) =
                        totals.iter_mut().find(|(name, _, _, _)| *name == arm)
                    {
                        aggregate.add(&report);
                        *rows += report.context_rows;
                        *count += 1;
                    } else {
                        let mut aggregate = Aggregate::default();
                        aggregate.add(&report);
                        totals.push((arm, aggregate, report.context_rows, 1));
                    }
                }
            }
            for (arm, aggregate, rows, count) in totals {
                points.push(Point {
                    arm,
                    capacity,
                    rot,
                    correct: aggregate.accuracy(),
                    decided: aggregate.decision_rate(),
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "an episode count that exceeds f64's mantissa is not a run anybody makes"
                    )]
                    rows: if count == 0 { 0.0 } else { rows / count as f64 },
                });
            }
        }
    }
    Ok(points)
}

/// Render the sweep as the table the question is actually asked in: one block
/// per `rot`, capacity down the side, arm across.
pub(crate) fn render(points: &[Point], rooms: usize) -> String {
    let mut out = String::new();
    let arms: Vec<&str> = {
        let mut seen: Vec<&str> = Vec::new();
        for point in points {
            if !seen.contains(&point.arm) {
                seen.push(point.arm);
            }
        }
        seen
    };

    let _ = writeln!(
        out,
        "context sweep over {rooms} rooms — correct %, decided %, and mean rows a member held\n"
    );
    let _ = writeln!(
        out,
        "`rot` is how hard the middle of the window is discounted: 0.0 charges only for\n\
         eviction, 1.0 is the strongest lost-in-the-middle curve this model expresses.\n\
         `window 0` is unbounded — the benchmark's existing behaviour, for comparison.\n"
    );

    for rot in ROTS {
        let block: Vec<&Point> = points
            .iter()
            .filter(|point| {
                (point.rot - rot).abs() < f64::EPSILON || (point.capacity == 0 && rot == ROTS[0])
            })
            .collect();
        if block.is_empty() {
            continue;
        }
        let _ = writeln!(out, "rot {rot:.1}");
        let mut header = format!("{:>8}", "window");
        for arm in &arms {
            let _ = write!(header, "{arm:>26}");
        }
        let _ = writeln!(out, "{header}");
        for capacity in CAPACITIES {
            if capacity == 0 && rot != ROTS[0] {
                continue;
            }
            let mut row = if capacity == 0 {
                format!("{:>8}", "∞")
            } else {
                format!("{capacity:>8}")
            };
            for arm in &arms {
                let cell = block
                    .iter()
                    .find(|point| point.arm == *arm && point.capacity == capacity)
                    .map_or_else(
                        || format!("{:>26}", "—"),
                        |point| {
                            format!(
                                "{:>12.1} {:>5.0}% {:>6.1}r",
                                point.correct, point.decided, point.rows
                            )
                        },
                    );
                let _ = write!(row, "{cell}");
            }
            let _ = writeln!(out, "{row}");
        }
        let _ = writeln!(out);
    }

    let _ = writeln!(
        out,
        "Read the ordering, not the numbers: `context.rs` models two published effects,\n\
         it does not measure a real model's retrieval. An ordering that holds across every\n\
         `rot` block is a claim about the protocols; one that changes hands between blocks\n\
         is a claim about the model, and should be reported as such."
    );
    out
}
