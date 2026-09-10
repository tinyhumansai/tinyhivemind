//! Unit tests for the grid: what the axes parse to, what they refuse, and
//! that a cell is reproducible from its own label.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::axes::{Axes, Complexity, Topic};
use super::{ARMS, CellRow, best, cell_seed, ratio};
use crate::cost::CostModel;
use crate::metrics::Aggregate;

fn args(values: &[&str]) -> impl Iterator<Item = String> + use<> {
    values
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>()
        .into_iter()
}

#[test]
fn the_default_axes_are_one_cell() {
    // The cheapest invocation has to be cheap: a benchmark whose smallest run
    // is sixty cells is one nobody runs before pushing.
    assert_eq!(Axes::point().cells().len(), 1);
}

#[test]
fn the_default_cell_reproduces_the_single_room_comparison() {
    let cells = Axes::point().cells();
    let cell = cells.first().expect("the default axes are one cell");
    assert_eq!(cell.topic, Topic::Uniform);
    assert_eq!(cell.scale, 5);
    assert_eq!(cell.complexity.topics(), 4);
    assert_eq!(cell.complexity.noise(), 90);
    assert_eq!(cell.concurrency, 1);
}

#[test]
fn the_cross_product_is_every_combination() {
    let mut axes = Axes::point();
    assert_eq!(
        axes.set("--topic", &mut args(&["uniform,hidden"])),
        Ok(true)
    );
    assert_eq!(axes.set("--scale", &mut args(&["3,5,9"])), Ok(true));
    assert_eq!(axes.set("--complexity", &mut args(&["1,5"])), Ok(true));
    assert_eq!(axes.set("--concurrency", &mut args(&["1,2,4"])), Ok(true));
    assert_eq!(axes.cells().len(), 2 * 3 * 2 * 3);
}

#[test]
fn adjacent_cells_differ_in_one_axis() {
    // The reading order is the point of the ordering: a reader compares a row
    // with the row beneath it whether or not they meant to, so those two rows
    // must not differ in two things at once.
    let mut axes = Axes::point();
    assert_eq!(axes.set("--concurrency", &mut args(&["1,2,4"])), Ok(true));
    let cells = axes.cells();
    for pair in cells.windows(2) {
        let (left, right) = (pair[0], pair[1]);
        assert_eq!(left.topic, right.topic);
        assert_eq!(left.scale, right.scale);
        assert_eq!(left.complexity, right.complexity);
        assert_ne!(left.concurrency, right.concurrency);
    }
}

#[test]
fn adjacent_cells_differ_in_one_axis_even_when_two_axes_wrap_together() {
    // A plain nested-loop product changes two coordinates the moment the
    // fastest axis wraps and the next one up advances -- e.g.
    // complexity=1,concurrency=4 followed by complexity=2,concurrency=1. The
    // single-axis case above cannot catch that; this widens both axes at
    // once so a wrap in the fast one lines up with a step in the slow one.
    let mut axes = Axes::point();
    assert_eq!(axes.set("--complexity", &mut args(&["1,2,5"])), Ok(true));
    assert_eq!(axes.set("--concurrency", &mut args(&["1,2,4"])), Ok(true));
    let cells = axes.cells();
    assert_eq!(cells.len(), 3 * 3);
    let mut changed_axes = Vec::new();
    for pair in cells.windows(2) {
        let (left, right) = (pair[0], pair[1]);
        let mut count = 0;
        if left.topic != right.topic {
            count += 1;
        }
        if left.scale != right.scale {
            count += 1;
        }
        if left.complexity != right.complexity {
            count += 1;
        }
        if left.concurrency != right.concurrency {
            count += 1;
        }
        changed_axes.push(count);
        assert_eq!(
            count, 1,
            "adjacent cells {left:?} -> {right:?} must differ in exactly one axis",
        );
    }
}

#[test]
fn all_takes_every_point_an_axis_defines() {
    let mut axes = Axes::point();
    assert_eq!(axes.set("--topic", &mut args(&["all"])), Ok(true));
    assert_eq!(axes.set("--complexity", &mut args(&["all"])), Ok(true));
    assert_eq!(axes.topics, Topic::ALL.to_vec());
    assert_eq!(axes.complexities, Complexity::ALL.to_vec());
}

#[test]
fn all_is_refused_on_an_axis_with_no_fixed_set_of_points() {
    let mut axes = Axes::point();
    assert!(axes.set("--scale", &mut args(&["all"])).is_err());
    assert!(axes.set("--concurrency", &mut args(&["all"])).is_err());
}

#[test]
fn refuses_a_point_that_is_not_on_its_axis() {
    // Skipping it instead would run a *smaller* grid than the operator asked
    // for and print it under the heading they typed -- the same failure an
    // unrecognised flag is refused for one level up.
    let mut axes = Axes::point();
    assert!(
        axes.set("--topic", &mut args(&["uniform,federated"]))
            .is_err()
    );
    assert!(axes.set("--complexity", &mut args(&["0"])).is_err());
    assert!(axes.set("--complexity", &mut args(&["6"])).is_err());
    assert!(axes.set("--scale", &mut args(&["1"])).is_err());
    // Bounded above too: `Room::generate_with` clamps to `MAX_MEMBERS`, so a
    // larger value would run a smaller room than the column header claims.
    let over = (crate::sim::MAX_MEMBERS + 1).to_string();
    assert!(axes.set("--scale", &mut args(&[&over])).is_err());
    let at_limit = crate::sim::MAX_MEMBERS.to_string();
    assert_eq!(axes.set("--scale", &mut args(&[&at_limit])), Ok(true));
    assert!(axes.set("--concurrency", &mut args(&["0"])).is_err());
    // Every refusal above left the axes as they were, so a run that stops on
    // a bad value never half-applied one.
    assert_eq!(axes.cells().len(), 1);
}

#[test]
fn refuses_a_missing_or_empty_list_rather_than_swallowing_the_next_flag() {
    let mut axes = Axes::point();
    assert!(axes.set("--topic", &mut args(&[])).is_err());
    assert!(axes.set("--scale", &mut args(&[",, ,"])).is_err());
}

#[test]
fn leaves_a_flag_it_does_not_own_for_somebody_else() {
    let mut axes = Axes::point();
    assert_eq!(axes.set("--seed", &mut args(&["3"])), Ok(false));
}

#[test]
fn a_cells_label_names_every_axis_that_produced_it() {
    let mut axes = Axes::point();
    assert_eq!(axes.set("--topic", &mut args(&["hidden"])), Ok(true));
    assert_eq!(axes.set("--scale", &mut args(&["9"])), Ok(true));
    assert_eq!(axes.set("--complexity", &mut args(&["4"])), Ok(true));
    assert_eq!(axes.set("--concurrency", &mut args(&["2"])), Ok(true));
    let cells = axes.cells();
    let cell = cells.first().expect("one cell");
    assert_eq!(
        cell.label(),
        "topic=hidden scale=9 complexity=4 concurrency=2"
    );
}

#[test]
fn each_cell_seeds_its_own_rooms() {
    // Two cells must be different rooms rather than the same rooms under
    // different labels, or every axis would look like it changed something.
    let mut axes = Axes::point();
    assert_eq!(axes.set("--topic", &mut args(&["all"])), Ok(true));
    assert_eq!(axes.set("--scale", &mut args(&["5,9"])), Ok(true));
    let cells = axes.cells();
    let mut seeds: Vec<u64> = cells.iter().map(cell_seed).collect();
    let before = seeds.len();
    seeds.sort_unstable();
    seeds.dedup();
    assert_eq!(seeds.len(), before, "two cells drew the same rooms");
}

#[test]
fn a_cells_seed_does_not_move_when_the_grid_around_it_does() {
    // What makes a single cell reproducible from its label: re-running one
    // cell alone has to draw the rooms it had inside the whole grid, or a
    // follow-up on an interesting cell measures a different sample.
    let mut wide = Axes::point();
    assert_eq!(wide.set("--topic", &mut args(&["all"])), Ok(true));
    assert_eq!(wide.set("--scale", &mut args(&["5,9,17"])), Ok(true));
    let alone = Axes::point().cells();
    let inside = wide.cells();

    let alone = alone.first().expect("one cell");
    let same = inside
        .iter()
        .find(|cell| cell.label() == alone.label())
        .expect("the default cell is inside the wider grid");
    assert_eq!(cell_seed(alone), cell_seed(same));
}

#[test]
fn a_wider_room_gets_proportionally_more_specialists() {
    // Fixing the count instead would make the expert topic quietly easier at
    // every larger scale, and the grid would report that as a property of
    // scale rather than of the topic.
    let small = Topic::Expert.expertise(6);
    let large = Topic::Expert.expertise(30);
    assert_ne!(small, large);
    // Never zero, however small the room: a topic with no expert in it is the
    // uniform topic under another name.
    assert_ne!(Topic::Expert.expertise(2), crate::sim::Expertise::Uniform);
}

#[test]
fn the_hidden_topic_overrides_the_levels_noise() {
    // At the default +-90 a lay member's argmax lands on the truth often
    // enough that a poll solves the profile by accident, and an arm beating a
    // control that already wins is measuring nothing.
    for level in Complexity::ALL {
        assert_eq!(Topic::Hidden.noise(level.noise()), 50);
        assert_eq!(Topic::Uniform.noise(level.noise()), level.noise());
    }
}

#[test]
fn complexity_rises_on_both_of_its_axes() {
    for pair in Complexity::ALL.windows(2) {
        assert!(pair[1].topics() > pair[0].topics());
        assert!(pair[1].noise() > pair[0].noise());
    }
}

#[test]
fn every_arm_has_a_name_and_they_are_all_different() {
    let mut names = ARMS.to_vec();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), ARMS.len());
}

/// Two arms tied for the best value of a metric must both be credited, not
/// just whichever sits first in [`ARMS`]. Exact ties are common for a
/// finite-sample accuracy and for a deterministic cost metric, so a strict
/// "first wins" comparison would silently misattribute a shared lead.
#[test]
fn best_credits_every_arm_tied_for_the_lead() {
    let rows: Vec<CellRow> = ARMS
        .iter()
        .map(|&name| CellRow {
            name,
            totals: Aggregate::priced_at(CostModel::DEFAULT),
        })
        .collect();
    // "ladder" and "hive+" tie for the largest value; everyone else trails.
    let value_of = |row: &CellRow| -> f64 {
        match row.name {
            "ladder" | "hive+" => 2.0,
            _ => 1.0,
        }
    };
    let mut led: Vec<[u32; 4]> = ARMS.iter().map(|_| [0; 4]).collect();
    best(&rows, &mut led, 0, value_of, true, true);
    let ladder = ARMS.iter().position(|&name| name == "ladder").unwrap();
    let hive_plus = ARMS.iter().position(|&name| name == "hive+").unwrap();
    let vote = ARMS.iter().position(|&name| name == "vote").unwrap();
    assert_eq!(led[ladder][0], 1, "the tied leader must be credited");
    assert_eq!(led[hive_plus][0], 1, "every tied leader must be credited");
    assert_eq!(led[vote][0], 0, "a trailing arm must not be credited");
}

#[test]
fn every_arm_leads_on_quality_when_every_arm_scores_zero() {
    // A quality of 0% is a real measurement -- every arm missing on a hard
    // cell -- and on such a cell they are all tied leaders. Filtering zero the
    // way an empty-sample latency is filtered credited nobody, which is the
    // one reading that is certainly wrong.
    let rows: Vec<CellRow> = ARMS
        .iter()
        .map(|&name| CellRow {
            name,
            totals: Aggregate::priced_at(CostModel::DEFAULT),
        })
        .collect();
    let mut led: Vec<[u32; 4]> = ARMS.iter().map(|_| [0; 4]).collect();
    best(&rows, &mut led, 0, |_| 0.0, true, true);
    assert!(
        led.iter().all(|counts| counts[0] == 1),
        "every arm tied at zero quality must be credited: {led:?}"
    );
}

#[test]
fn an_arm_that_never_ran_does_not_lead_on_speed() {
    // The other side of the same rule: a latency, a throughput or a token
    // count of zero means the sample was empty, and an arm that never ran is
    // not the fast one.
    let rows: Vec<CellRow> = ARMS
        .iter()
        .map(|&name| CellRow {
            name,
            totals: Aggregate::priced_at(CostModel::DEFAULT),
        })
        .collect();
    let mut led: Vec<[u32; 4]> = ARMS.iter().map(|_| [0; 4]).collect();
    best(&rows, &mut led, 1, |_| 0.0, false, false);
    assert!(
        led.iter().all(|counts| counts[1] == 0),
        "an empty sample must not be credited as fastest: {led:?}"
    );
}

#[test]
fn a_ratio_against_nothing_is_zero_rather_than_infinite() {
    assert!((ratio(5.0, 0.0) - 0.0).abs() < f64::EPSILON);
    assert!((ratio(5.0, f64::NAN) - 0.0).abs() < f64::EPSILON);
    assert!((ratio(6.0, 3.0) - 2.0).abs() < f64::EPSILON);
}
