//! Unit tests for the horizon benchmark.
//!
//! Promoted out of [`super`] to keep implementation and test module
//! files separate, matching the house convention (see `variety/`).

use super::*;
use crate::sim::{Expertise, SimAgent};

/// Compare two percentages, which arrive through floating-point division.
fn close(left: f64, right: f64) -> bool {
    (left - right).abs() < 1e-9
}

/// A room for one stage of a chain, at the defaults the sweep uses.
fn room(stage: usize) -> Room {
    Room::generate_with(0xA11CE, 5, 4, 50, Expertise::Uniform, false).for_stage(stage)
}

/// The property the whole chain rests on: no reading taken at one stage
/// can be scored against another stage's question.
#[test]
fn every_stage_names_its_options_differently() {
    let first = room(0);
    let second = room(1);
    let names = |held: &Room| -> Vec<String> {
        held.agents
            .first()
            .map(|agent| {
                agent
                    .evals
                    .iter()
                    .map(|(topic, _)| topic.0.clone())
                    .collect()
            })
            .unwrap_or_default()
    };
    let (early, late) = (names(&first), names(&second));
    assert!(!early.is_empty(), "a stage has options");
    for name in &early {
        assert!(
            !late.contains(name),
            "option {name} appears at two stages and could be scored at the wrong one"
        );
    }
    assert_ne!(first.truth, second.truth);
}

/// A member carries what it held at the previous stage into this one.
#[test]
fn a_later_stage_inherits_the_window_of_an_earlier_one() {
    let mut first = room(0);
    first.charge_brief();
    let carried = first.agents.first().map_or(0, SimAgent::held);
    assert!(carried > 0, "the brief occupies rows");

    let mut second = room(1).inheriting(&first);
    second.charge_brief();
    let held = second.agents.first().map_or(0, SimAgent::held);
    assert_eq!(
        held,
        carried * 2,
        "stage two carries stage one's rows as well as its own"
    );
}

/// Poisoning lifts one decoy and never the truth, so a poisoned chain is
/// harder rather than unwinnable.
#[test]
fn poison_lifts_a_decoy_and_leaves_the_truth_alone() {
    let clean = room(0);
    let poisoned = clean.poisoned(POISON_LIFT);
    let truth = clean.truth.clone();
    let reading = |held: &Room, topic: &tinyhivemind_hive::trace::TopicId| {
        held.agents
            .first()
            .map_or(0, |agent| agent.own_reading(topic))
    };
    assert_eq!(
        reading(&clean, &truth),
        reading(&poisoned, &truth),
        "the truth is never lifted"
    );
    let lifted = clean.agents.first().and_then(|agent| {
        agent
            .evals
            .iter()
            .map(|(topic, _)| topic)
            .find(|topic| **topic != truth)
            .cloned()
    });
    assert!(lifted.is_some(), "a room of four options has a decoy");
    if let Some(lifted) = lifted {
        assert_eq!(
            reading(&poisoned, &lifted) - reading(&clean, &lifted),
            POISON_LIFT,
        );
    }
}

/// A soloist handed every peer's brief carries `agents` times what one
/// member of the room does. That ratio is the whole quantity under test.
#[test]
fn a_soloist_carries_the_whole_room_s_brief() {
    let mut shared = room(0);
    shared.charge_brief();
    let one = shared.agents.first().map_or(0, SimAgent::held);
    let pooled = shared.pooled();
    let alone = pooled.agents.first().map_or(0, SimAgent::held);
    assert!(
        alone > one,
        "a soloist holding every peer's readings carries more than one member: {alone} vs {one}"
    );
}

/// End-to-end can never exceed the per-stage rate, and equals it at one
/// stage — the accounting invariant the headline column rests on.
#[test]
fn a_whole_chain_is_never_likelier_than_one_of_its_stages() {
    let mut chain = Chain::default();
    chain.add(&ChainRun {
        stages: 4,
        right: 3,
        ..ChainRun::default()
    });
    chain.add(&ChainRun {
        stages: 4,
        right: 4,
        ..ChainRun::default()
    });
    assert!(chain.end_to_end() <= chain.per_stage());
    assert!(close(chain.end_to_end(), 50.0));
    assert!(close(chain.per_stage(), 87.5));

    let mut single = Chain::default();
    single.add(&ChainRun {
        stages: 1,
        right: 1,
        ..ChainRun::default()
    });
    assert!(close(single.end_to_end(), single.per_stage()));
}

/// `staged` bypasses the normal room-generation path `main.rs` applies
/// `--blind-evidence` on, so it must apply the flag itself rather than
/// silently building every member with the default, off, value.
#[test]
fn a_staged_room_applies_blind_evidence() {
    let options = Options {
        blind_evidence: true,
        ..Options::defaults()
    };
    let room = staged(&options, 0xA11CE, 0, None, false);
    assert!(
        room.agents.iter().all(SimAgent::opens_with_evidence),
        "--blind-evidence must reach every member of a staged room, \
         the same way it reaches the default room-generation path",
    );

    let off = staged(&Options::defaults(), 0xA11CE, 0, None, false);
    assert!(
        off.agents.iter().all(|agent| !agent.opens_with_evidence()),
        "a staged room built without the flag stays off, exactly as \
         the default room does",
    );
}
