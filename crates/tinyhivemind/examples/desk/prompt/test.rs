//! Unit tests for what one seat is told before it takes a turn.

#![allow(clippy::expect_used, clippy::panic, clippy::unwrap_used)]

use super::*;
use tinyhivemind::SessionAuthor;

fn seats() -> Vec<deskfile::AgentSpec> {
    ["theory", "solver", "checker"]
        .into_iter()
        .map(|id| deskfile::AgentSpec {
            id: id.into(),
            label: id.into(),
            role: format!("{id} role"),
            brief: format!("{id} brief"),
        })
        .collect()
}

#[test]
fn names_every_seat_and_marks_the_one_reading_it() {
    let spoken = BTreeMap::from([("theory".to_string(), 7_u64)]);
    let text = who_is_here(&seats(), "solver", &spoken);
    assert!(text.contains("@theory — theory role — last spoke at [7]"));
    assert!(
        text.contains("@solver (you)"),
        "the reader is marked: {text}"
    );
    assert!(
        text.contains("@checker — checker role — has not spoken yet"),
        "a silent seat is named as silent, which is the point: {text}"
    );
}

#[test]
fn says_that_naming_a_seat_is_what_runs_it() {
    let text = who_is_here(&seats(), "theory", &BTreeMap::new());
    assert!(
        text.contains("is what runs them next"),
        "a seat that does not know this ends the chain: {text}"
    );
}

#[test]
fn a_desk_where_nobody_has_spoken_still_lists_everybody() {
    let text = who_is_here(&seats(), "lead", &BTreeMap::new());
    for id in ["@theory", "@solver", "@checker"] {
        assert!(text.contains(id), "{id} missing from {text}");
    }
    assert!(!text.contains("(you)"), "no seat here is the reader");
}

fn message(sequence: u64, text: &str) -> SessionMessage {
    SessionMessage {
        sequence: tinyhivemind::Sequence(sequence),
        author: SessionAuthor::Agent {
            id: "theory".into(),
            label: "Theory".into(),
        },
        content: text.into(),
        audience: tinyhivemind::aside::Audience::Desk,
        elided: None,
    }
}

#[test]
fn puts_the_account_first_and_says_it_is_lossy() {
    let text = desk_so_far(Some("  Psi(3)=20302 verified at [4].  "), &[]);
    assert!(text.starts_with("\n\n## The desk so far (read this first)\n"));
    assert!(text.contains("Psi(3)=20302 verified at [4]."), "{text}");
    assert!(
        text.contains("is lossy"),
        "a seat must not trust it blindly"
    );
    assert!(
        text.contains("desk_read(limit)"),
        "and must be told how to reach the real messages: {text}"
    );
}

#[test]
fn says_so_plainly_when_no_account_has_been_written() {
    let text = desk_so_far(None, &[message(3, "opening")]);
    assert!(
        text.contains("No standing account has been written yet"),
        "{text}"
    );
    assert!(
        text.contains("starting at [3]"),
        "it names where the room actually starts: {text}"
    );
}

#[test]
fn an_empty_desk_still_gets_a_section_rather_than_silence() {
    let text = desk_so_far(None, &[]);
    assert!(text.contains("the opening message"), "{text}");
    assert!(
        text.contains("desk_read(limit)"),
        "the way further back is always offered: {text}"
    );
}
