//! One benchmark over a cross product of four axes, in one table shape.
//!
//! # Why this replaced five sweeps
//!
//! The questions a host actually asks are *how does this behave on my kind of
//! problem, at my size, at my difficulty, at the concurrency I can afford* —
//! and those are one question with four parameters, not four questions. The
//! harness answered them as four: `--scale-sweep` walked room size,
//! `--context-sweep` walked the window, `--stages` walked the horizon,
//! `--facets` walked the width, and the plain comparison held every axis
//! fixed. Each printed a table of its own shape, so no two of them could be
//! read against each other, and the axis a finding actually depended on was
//! never visible in the table that reported it.
//!
//! This module walks the cross product instead. Every cell runs the same arms
//! and reports the same six columns, so a row and the row beneath it differ in
//! exactly one axis value and the comparison is the reader's to make rather
//! than the author's to assert.
//!
//! # What is measured
//!
//! The six columns of [`crate::metrics::arm_row`], for every arm in every
//! cell: quality, speed, throughput, concurrency, tokens per episode and
//! tokens per second. Five of the six come from [`crate::cost`], which is a
//! model — read `cost.rs`'s own header for what it does and does not claim,
//! and re-run with the constants moved before believing anything that turns on
//! them.
//!
//! # What a cell costs to run
//!
//! Every arm, over `--episodes` rooms, per cell. The default axes are a single
//! point for that reason: widening all four at once multiplies, and a
//! four-by-four-by-five-by-three grid is 240 cells. `--jobs` spreads the rooms
//! inside a cell across cores; cells themselves run in order, so the output
//! streams and a long run can be read before it finishes.

mod axes;

pub(crate) use axes::{Axes, Cell};

use crate::arms;
use crate::cli::Options;
use crate::metrics::{Aggregate, arm_row, wilson};
use crate::parallel;
use crate::policy::{tuned_policy, widened_policy};
use crate::rng::mix;
use crate::run::run_episode;
use crate::sim::Room;

/// The task every room in the grid is given.
const TASK: &str = "We must choose one rollout strategy. Decide.";

/// The arms every grid cell runs, in printing order.
///
/// Five systems rather than the single-point comparison's twenty-four. The
/// twenty-four are *mechanism probes* — each asks whether one move is worth
/// its turns, against a control differing from it in exactly one thing — and
/// reproducing that table in every cell would print hundreds of rows to answer
/// a question about four axes. These five are the systems a host actually
/// chooses between:
///
/// - `ladder` — route to one member and take its answer. One turn.
/// - `vote` — independent answers, plurality, matched budget. One round.
/// - `hive+` — a deliberation at the tuned policy, strictly sequential.
/// - `hive+wide` — the same, in rounds of the cell's `concurrency`.
/// - `hive+pooled` — every member handed every peer's reading for free. Not a
///   system anybody can deploy; it is the ceiling the other four are read
///   against, and it is in the table so that a gap to it is visible rather
///   than assumed away.
///
/// Run the default `bench` with no `--grid` for the mechanism probes; they
/// answer a different question and they answer it at one point.
const ARMS: [&str; 5] = ["ladder", "vote", "hive+", "hive+wide", "hive+pooled"];

/// One arm's totals in one cell.
struct CellRow {
    /// The arm's name, as the table prints it.
    name: &'static str,
    /// What it scored.
    totals: Aggregate,
}

/// Run the whole grid and print it.
///
/// # Errors
///
/// Returns the library's own error text from any arm in any cell.
pub(crate) fn run(options: &Options) -> Result<(), String> {
    println!("{}\n", options.cost_model.header());
    let cells = options.axes.cells();
    println!(
        "grid: {} cell{} x {} rooms, seed {}\n",
        cells.len(),
        if cells.len() == 1 { "" } else { "s" },
        options.episodes,
        options.seed,
    );

    println!("{}", header());
    let mut every: Vec<(Cell, Vec<CellRow>)> = Vec::new();
    for cell in cells {
        let rows = run_cell(options, cell)?;
        for row in &rows {
            println!("{}{}", cell.columns(), arm_row(row.name, &row.totals));
        }
        println!();
        every.push((cell, rows));
    }

    if every.len() > 1 {
        summarise(&every);
    }
    Ok(())
}

/// The header for the grid table: the four axes, then the six metrics.
fn header() -> String {
    format!(
        "{:<9}{:>6}{:>6}{:>6}  {}",
        "topic",
        "scale",
        "cplx",
        "conc",
        crate::metrics::arm_header(),
    )
}

/// Run every arm of one cell over the cell's own rooms.
///
/// # Errors
///
/// Returns the library's own error text from any arm.
fn run_cell(options: &Options, cell: Cell) -> Result<Vec<CellRow>, String> {
    let tuned = tuned_policy(cell.scale);
    let wide = widened_policy(&tuned, cell.concurrency);
    // Seeded from the cell as well as the run, so two cells are different
    // rooms rather than the same rooms under different labels -- and so that
    // re-running one cell alone reproduces exactly the rooms it had inside the
    // whole grid.
    let seed = mix(options.seed, cell_seed(&cell));
    let rooms: Vec<Room> = (0..options.episodes)
        .map(|index| {
            Room::generate_with(
                mix(seed, u64::from(index)),
                cell.scale,
                cell.complexity.topics(),
                cell.topic.noise(cell.complexity.noise()),
                cell.topic.expertise(cell.scale),
                false,
            )
        })
        .collect();

    let indexed: Vec<(usize, &Room)> = rooms.iter().enumerate().collect();
    let per_room: Vec<Vec<Aggregate>> =
        parallel::map_in_order(&indexed, options.jobs, |(index, room)| {
            let (index, room) = (*index, *room);
            let mut fold: Vec<Aggregate> = ARMS
                .iter()
                .map(|_| Aggregate::priced_at(options.cost_model))
                .collect();
            let room_seed = mix(seed, u64::try_from(index).unwrap_or(0));
            let mut at = 0_usize;
            let mut next = || {
                let index = at;
                at += 1;
                index
            };

            if let Some(slot) = fold.get_mut(next()) {
                slot.add_arm(&arms::run_ladder(room, room_seed)?);
            }
            if let Some(slot) = fold.get_mut(next()) {
                slot.add_arm(&arms::run_vote(room, tuned.turn_budget));
            }
            if let Some(slot) = fold.get_mut(next()) {
                slot.add(&run_episode(room, &tuned, TASK, false)?);
            }
            if let Some(slot) = fold.get_mut(next()) {
                slot.add(&run_episode(room, &wide, TASK, false)?);
            }
            if let Some(slot) = fold.get_mut(next()) {
                slot.add(&run_episode(&room.pooled(), &tuned, TASK, false)?);
            }
            Ok(fold)
        })?;

    let mut totals: Vec<Aggregate> = ARMS
        .iter()
        .map(|_| Aggregate::priced_at(options.cost_model))
        .collect();
    for chunk in &per_room {
        for (mine, theirs) in totals.iter_mut().zip(chunk) {
            mine.merge(theirs);
        }
    }
    Ok(ARMS.iter()
        .copied()
        .zip(totals)
        .map(|(name, totals)| CellRow { name, totals })
        .collect())
}

/// A stable seed for one cell, so re-running it alone reproduces the rooms it
/// had inside the whole grid.
fn cell_seed(cell: &Cell) -> u64 {
    let mut seed = mix(0x6365_6C6C, cell.topic as u64);
    seed = mix(seed, cell.scale as u64);
    seed = mix(seed, u64::from(cell.complexity.0));
    mix(seed, u64::from(cell.concurrency))
}

/// Say which arm led on each metric across the whole grid, and where.
///
/// A grid of any size is too many numbers to rank by eye, and the failure mode
/// of leaving it unranked is that a reader finds the cell that agrees with
/// them. This says plainly how many cells each arm led in, per metric — and
/// when the lead is split it says that too, which is usually the finding.
fn summarise(every: &[(Cell, Vec<CellRow>)]) {
    println!("across {} cells, cells led per arm", every.len());
    println!(
        "{:<16}{:>10}{:>10}{:>11}{:>10}",
        "arm", "quality", "speed", "tok/ep", "thru"
    );

    let mut led: Vec<[u32; 4]> = ARMS.iter().map(|_| [0; 4]).collect();
    for (_, rows) in every {
        // Quality and throughput are won by the largest; speed and tokens by
        // the smallest. An arm that decided nothing is not a fast arm, so a
        // zero latency -- which only an empty sample produces -- never wins.
        best(rows, &mut led, 0, |row| row.totals.accuracy(), true);
        best(rows, &mut led, 1, |row| row.totals.latency_ms(), false);
        best(rows, &mut led, 2, |row| row.totals.tokens_per_episode(), false);
        best(rows, &mut led, 3, |row| row.totals.episodes_per_hour(), true);
    }
    for (name, counts) in ARMS.iter().zip(&led) {
        println!(
            "{name:<16}{:>10}{:>10}{:>11}{:>10}",
            counts[0], counts[1], counts[2], counts[3]
        );
    }

    // The one comparison worth making for the reader rather than leaving to
    // them, because it is the one the six columns exist to enable: an arm that
    // is no better than the cheap control is not worth its tokens, however
    // good its absolute number looks.
    println!();
    for (cell, rows) in every {
        let Some(vote) = rows.iter().find(|row| row.name == "vote") else {
            continue;
        };
        for row in rows.iter().filter(|row| row.name != "vote") {
            let gap = row.totals.accuracy() - vote.totals.accuracy();
            let (low, high) = wilson(row.totals.correct, row.totals.episodes);
            let (vote_low, vote_high) = wilson(vote.totals.correct, vote.totals.episodes);
            // Overlapping Wilson intervals is a conservative reading of "not
            // separated", and deliberately the conservative one: this line
            // exists to stop a reader claiming a difference the sample cannot
            // support, so it should under-claim rather than over-claim.
            let separated = low > vote_high || high < vote_low;
            if separated && gap > 0.0 {
                println!(
                    "{}: {} beats vote by {gap:.1} points, at {:.1}x the tokens",
                    cell.label(),
                    row.name,
                    ratio(
                        row.totals.tokens_per_episode(),
                        vote.totals.tokens_per_episode()
                    ),
                );
            }
        }
    }
}

/// Credit the arm with the best value of one metric in one cell.
fn best(
    rows: &[CellRow],
    led: &mut [[u32; 4]],
    metric: usize,
    of: impl Fn(&CellRow) -> f64,
    largest: bool,
) {
    let mut winner: Option<(usize, f64)> = None;
    for (index, row) in rows.iter().enumerate() {
        let value = of(row);
        if !value.is_finite() || value <= 0.0 {
            continue;
        }
        let better = winner.is_none_or(|(_, best)| if largest { value > best } else { value < best });
        if better {
            winner = Some((index, value));
        }
    }
    if let Some((index, _)) = winner
        && let Some(counts) = led.get_mut(index)
        && let Some(count) = counts.get_mut(metric)
    {
        *count = count.saturating_add(1);
    }
}

/// `numerator / denominator`, or zero when the denominator is not usable.
fn ratio(numerator: f64, denominator: f64) -> f64 {
    if denominator <= 0.0 || !denominator.is_finite() {
        return 0.0;
    }
    numerator / denominator
}

#[cfg(test)]
mod test;
