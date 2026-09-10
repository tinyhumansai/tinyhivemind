//! Unit tests for the two clauses a desk can put on the wire, and the
//! guarantee that adding one does not disturb the other.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

#[test]
fn a_reading_clause_round_trips() {
    let line = "Desk-1 reads #stage at 40, #ship at 100.";
    let held = readings(line);
    assert_eq!(held.len(), 2);
    assert_eq!(held[0].desk, "Desk-1");
    assert_eq!(held[0].topic.to_string(), "stage");
    assert_eq!(held[0].value, 40);
    assert_eq!(restate(&held), line);
}

#[test]
fn a_fact_clause_round_trips() {
    let line = "Desk-1 rules out #stage.";
    let held = facts(line);
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].desk, "Desk-1");
    assert_eq!(held[0].topic.to_string(), "stage");
    assert_eq!(restate_facts(&held), line);
}

#[test]
fn a_fact_clause_leaves_the_readings_alone() {
    // The guarantee that keeps every recorded number taken without
    // `--evidence` reproducible: a line that gained a fact clause is read by
    // the reading parser exactly as the line without it was.
    let plain = "Desk-1 reads #stage at 40, #ship at 100.";
    let carrying = "Desk-1 reads #stage at 40, #ship at 100. Desk-1 rules out #stage.";

    let before = readings(plain);
    let after = readings(carrying);
    assert_eq!(before.len(), after.len());
    for (before, after) in before.iter().zip(&after) {
        assert_eq!(before.desk, after.desk);
        assert_eq!(before.topic, after.topic);
        assert_eq!(before.value, after.value);
    }
}

#[test]
fn a_reading_clause_carries_no_facts() {
    // The other direction, and the one that would silently disqualify every
    // option a desk mentioned: an option named with a rating is an opinion,
    // never a disqualification.
    assert!(facts("Desk-1 reads #stage at 40, #ship at 100.").is_empty());
}

#[test]
fn a_reads_clause_closes_an_open_fact_clause() {
    // Both clauses in one line, in either order, and neither leaks into the
    // other's options.
    let line = "Desk-1 rules out #stage. Desk-2 reads #ship at 90, #stage at 10.";
    let held = facts(line);
    assert_eq!(held.len(), 1, "only #stage is ruled out: {held:?}");
    assert_eq!(held[0].desk, "Desk-1");
    assert_eq!(held[0].topic.to_string(), "stage");

    let opinions = readings(line);
    assert_eq!(opinions.len(), 2);
    assert!(opinions.iter().all(|reading| reading.desk == "Desk-2"));
}

#[test]
fn several_desks_rule_out_several_options() {
    let line = "Desk-1 rules out #stage, #ship. Desk-2 rules out #canary.";
    let held = facts(line);
    assert_eq!(held.len(), 3);
    assert_eq!(held[0].desk, "Desk-1");
    assert_eq!(held[2].desk, "Desk-2");
    assert_eq!(held[2].topic.to_string(), "canary");
    assert_eq!(restate_facts(&held), line);
}

#[test]
fn a_disqualification_nobody_owns_is_dropped() {
    // A bare `#option` with no desk clause open names nothing that can be
    // attributed, and an unattributable fact must not be applied — the same
    // rule the reading parser follows.
    assert!(facts("we should drop #stage entirely.").is_empty());
}
