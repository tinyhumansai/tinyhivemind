//! Tests for the scenario file format.
//!
//! What is checked here is that every scenario this harness ships actually
//! parses and validates. A live corpus takes minutes per scenario against a
//! real endpoint, so a fixture with a typo in it is otherwise found by a run
//! that has already spent the money.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::Scenario;

/// Every `.txt` under `scenarios/`, read at compile time.
///
/// Listed rather than globbed because `include_str!` needs a literal, and a
/// missing entry here is caught by the count assertion below rather than by
/// nobody noticing.
const SHIPPED: [(&str, &str); 7] = [
    (
        "assay-drift",
        include_str!("../scenarios/assay-drift.txt"),
    ),
    (
        "chargeback-spike",
        include_str!("../scenarios/chargeback-spike.txt"),
    ),
    (
        "checkout-503",
        include_str!("../scenarios/checkout-503.txt"),
    ),
    (
        "checkout-503-federated",
        include_str!("../scenarios/checkout-503-federated.txt"),
    ),
    (
        "index-lock-expert",
        include_str!("../scenarios/index-lock-expert.txt"),
    ),
    (
        "index-lock-tiers",
        include_str!("../scenarios/index-lock-tiers.txt"),
    ),
    (
        "port-congestion",
        include_str!("../scenarios/port-congestion.txt"),
    ),
];

#[test]
fn every_shipped_scenario_parses() {
    for (name, text) in SHIPPED {
        let scenario = Scenario::parse(text)
            .unwrap_or_else(|error| panic!("{name} does not parse: {error}"));
        assert!(
            scenario.options.len() >= 2,
            "{name} needs options to choose between"
        );
        assert!(
            scenario.member_ids().len() >= 2,
            "{name} needs a room, not a member"
        );
    }
}

#[test]
fn every_shipped_scenario_records_an_answer_that_is_on_offer() {
    // The recorded truth has to be one of the options, or the scenario scores
    // every room wrong however well it deliberated.
    for (name, text) in SHIPPED {
        let scenario = Scenario::parse(text).expect("it parses");
        assert!(
            scenario
                .options
                .iter()
                .any(|option| option.id == scenario.truth),
            "{name} records a truth that is not one of its options"
        );
    }
}

#[test]
fn every_member_holds_something_of_its_own() {
    // A hidden profile is only hidden if the members differ. A seat with no
    // private brief is a seat that contributes nothing the others could not
    // already read, which is worth failing rather than shipping.
    for (name, text) in SHIPPED {
        let scenario = Scenario::parse(text).expect("it parses");
        for agent in &scenario.agents {
            assert!(
                !agent.knows.is_empty(),
                "{name}: {} holds nothing private",
                agent.id
            );
        }
    }
}
