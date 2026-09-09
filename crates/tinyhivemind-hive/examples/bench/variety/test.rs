//! Unit tests for the variety sweep.
//!
//! Promoted out of [`super`] when the sweep passed the seven-hundred-line file
//! cap. The seam is the house one: what the module *does* stays in `mod.rs`,
//! and what proves it does that lives here.

use tinyhivemind_hive::trace::TopicId;

use super::*;
use crate::sim::SimAgent;

/// Compare two percentages, which arrive through floating-point division.
fn close(left: f64, right: f64) -> bool {
    (left - right).abs() < 1e-9
}

/// A room for one facet of a task, at the defaults the sweep uses.
fn room(facet: usize, expertise: Expertise) -> Room {
    Room::generate_with(0xFACE7, 5, 4, 50, expertise, false).for_facet(facet)
}

/// The option names one member of a room holds a reading of.
fn names(held: &Room, index: usize) -> Vec<String> {
    held.agents.get(index).map_or_else(Vec::new, |agent| {
        agent
            .evals
            .iter()
            .map(|(topic, _)| topic.0.clone())
            .collect()
    })
}

/// The property the whole spread rests on: no reading taken on one facet
/// can be scored against another facet's question.
#[test]
fn every_facet_names_its_options_differently() {
    let first = room(0, Expertise::Uniform);
    let second = room(1, Expertise::Uniform);
    let (early, late) = (names(&first, 0), names(&second, 0));
    assert!(!early.is_empty(), "a facet has options");
    for name in &early {
        assert!(
            !late.contains(name),
            "option {name} appears on two facets and could be scored on the wrong one"
        );
    }
}

/// A facet's options cannot collide with a stage's either, so a run that
/// ever combined depth and width could not score one against the other.
#[test]
fn a_facet_never_names_an_option_the_way_a_stage_does() {
    let base = Room::generate_with(0xFACE7, 5, 4, 50, Expertise::Uniform, false);
    let staged = names(&base.for_stage(1), 0);
    for name in names(&base.for_facet(1), 0) {
        assert!(
            !staged.contains(&name),
            "facet option {name} collides with a stage's"
        );
    }
}

/// Under `--roles` the owner is fixed by construction rather than drawn,
/// so the harness knows whose facet it is before anybody reads anything.
#[test]
fn an_owned_facet_names_its_owner_by_construction() {
    for owner in 0..5 {
        let held = room(0, Expertise::Roles { owner });
        let expected = crate::sim::member_at(owner).0;
        assert_eq!(
            held.deciding_expert(),
            Some(expected.as_str()),
            "member {owner} owns every option of its own facet"
        );
    }
}

/// An owner reads its own facet more tightly than a peer does. Stated as
/// a spread over the room rather than as one draw, because a single noisy
/// reading proves nothing either way.
#[test]
fn an_owner_reads_its_facet_more_tightly_than_its_room_does() {
    let held = room(0, Expertise::Roles { owner: 0 });
    let truth = held.truth.clone();
    let error = |index: usize| {
        held.agents
            .get(index)
            .map_or(i32::MAX, |agent| (agent.own_reading(&truth) - 100).abs())
    };
    let owner = error(0);
    let lay: i32 = (1..held.agents.len()).map(error).sum();
    let mean = lay / i32::try_from(held.agents.len() - 1).unwrap_or(1);
    assert!(
        owner < mean,
        "the owner's error {owner} is not tighter than the room's mean {mean}"
    );
}

/// A seat holding one facet of a task carries less than a soloist holding
/// all of them. That ratio is the whole quantity under test.
#[test]
fn a_seat_carries_one_facet_where_a_soloist_carries_every_facet() {
    let mut first = room(0, Expertise::Uniform);
    first.charge_brief();
    let one = first.held_by(0);
    assert!(one > 0, "the brief occupies rows");

    // A soloist goes on to the next facet still holding the last one.
    let mut second = room(1, Expertise::Uniform).inheriting(&first);
    second.charge_brief();
    assert_eq!(
        second.held_by(0),
        one * 2,
        "a soloist on its second facet carries the first one as well"
    );

    // A seat given only its own facet inherits nothing, because the facet
    // before it belonged to somebody else.
    let mut owned = room(1, Expertise::Uniform);
    owned.charge_brief();
    assert_eq!(owned.held_by(0), one, "a seat carries only its own facet");
}

/// `held_by` reports one seat rather than the room's mean, which is the
/// distinction the split arm's load column rests on.
#[test]
fn held_by_reads_one_seat_and_not_the_rooms_mean() {
    let mut held = room(0, Expertise::Uniform);
    held.charge_brief();
    if let Some(agent) = held.agents.first_mut() {
        agent.note_stub(&TopicId::from("extra"));
    }
    let mean = held.held();
    let first = held.held_by(0);
    let second = held.held_by(1);
    assert!(first > second, "the seat given an extra row holds more");
    assert!(
        as_f64(u64::try_from(first).unwrap_or(0)) > mean,
        "one busy seat is understated by the room's mean"
    );
    assert_eq!(
        held.held_by(usize::MAX),
        0,
        "a seat nobody fills holds nothing"
    );
}

/// The depth the concurrency buys: independent facets ride one round, so
/// a room of `n` answers `n` of them for the wall clock of one.
#[test]
fn a_split_task_is_shallower_than_a_serial_one() {
    for (facets, seats, expected) in [(1_usize, 5_usize, 1_usize), (4, 5, 1), (8, 5, 2)] {
        assert_eq!(facets.div_ceil(seats), expected);
        assert!(
            facets.div_ceil(seats) <= facets,
            "splitting a task can never make it deeper than working it alone"
        );
    }
}

/// All-facets can never exceed the per-facet rate, and equals it at one
/// facet — the accounting invariant the headline column rests on.
#[test]
fn a_whole_task_is_never_likelier_than_one_of_its_facets() {
    let mut spread = Spread::default();
    spread.add(&TaskRun {
        facets: 4,
        right: 3,
        ..TaskRun::default()
    });
    spread.add(&TaskRun {
        facets: 4,
        right: 4,
        ..TaskRun::default()
    });
    assert!(spread.all_facets() <= spread.per_facet());
    assert!(close(spread.all_facets(), 50.0));
    assert!(close(spread.per_facet(), 87.5));

    let mut single = Spread::default();
    single.add(&TaskRun {
        facets: 1,
        right: 1,
        ..TaskRun::default()
    });
    assert!(close(single.all_facets(), single.per_facet()));
}

/// A member that owns no facet is left out of the load average rather
/// than counted as an idle seat holding nothing.
#[test]
fn a_seat_given_no_facet_is_not_averaged_in() {
    let mut held = room(0, Expertise::Uniform);
    held.charge_brief();
    let rows = held.held_by(0);
    assert!(rows > 0);
    let one_seat = ratio_f64(as_f64(u64::try_from(rows).unwrap_or(0)), 1);
    let two_seats = ratio_f64(as_f64(u64::try_from(rows).unwrap_or(0)), 2);
    assert!(
        one_seat > two_seats,
        "counting an idle seat would halve the load the arm actually carries"
    );
    assert!(
        close(ratio_f64(1.0, 0), 0.0),
        "no seats is no load, not a divide by zero"
    );
}

/// `SimAgent` is named here so the import above is not dead weight: the
/// load columns read rows off one, and a change to what a row is should
/// break this file rather than silently move a published number.
#[test]
fn a_charged_brief_is_one_row_per_option() {
    let mut held = room(0, Expertise::Uniform);
    let before = held.agents.first().map_or(0, SimAgent::held);
    held.charge_brief();
    let after = held.agents.first().map_or(0, SimAgent::held);
    assert_eq!(after - before, names(&held, 0).len());
}

/// Every room-shaping flag a sweep generates its own rooms under has to be
/// carried by that sweep's constructor. `--blind-evidence` was dropped here,
/// so a run that asked for it got a room that had never heard of it — which
/// reads as a result rather than as an omission.
#[test]
fn a_faceted_room_applies_blind_evidence() {
    let mut options = Options::defaults();
    options.blind_evidence = true;
    let room = faceted(&options, 0xFACE7, 0, None);
    assert!(
        room.agents.iter().all(SimAgent::opens_with_evidence),
        "every member of a faceted room opens on a deposit when asked to"
    );

    let plain = faceted(&Options::defaults(), 0xFACE7, 0, None);
    assert!(
        plain.agents.iter().all(|agent| !agent.opens_with_evidence()),
        "and none of them does when nobody asked, so no published number moves"
    );
}
