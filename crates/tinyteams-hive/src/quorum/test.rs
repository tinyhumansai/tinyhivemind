//! Unit tests for quorum counting and cross-inhibition.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

use crate::trace::read;
use tinyteams::{SessionAuthor, SessionMessage};

fn said(sequence: u64, author: &str, content: &str) -> SessionMessage {
    SessionMessage {
        sequence: Sequence(sequence),
        author: SessionAuthor::Agent {
            id: author.into(),
            label: author.into(),
        },
        content: content.into(),
    }
}

fn policy(threshold: u32) -> QuorumPolicy {
    QuorumPolicy {
        threshold,
        window: 100,
        require_grounded: true,
    }
}

fn fold(transcript: &[SessionMessage], policy: &QuorumPolicy) -> Vec<TopicStanding> {
    let at = transcript.last().map_or(Sequence(0), |m| m.sequence);
    standings(&read(transcript), at, policy).expect("folds")
}

fn standing<'a>(standings: &'a [TopicStanding], topic: &str) -> &'a TopicStanding {
    standings
        .iter()
        .find(|standing| standing.topic.as_str() == topic)
        .unwrap_or_else(|| panic!("no standing for {topic}"))
}

/// Two proposals, two grounded supporters each: a genuine tie.
fn deadlocked_transcript() -> Vec<SessionMessage> {
    vec![
        said(1, "planner", "!propose #stage Stage the rollout."),
        said(2, "scout", "!propose #ship Ship it at once."),
        said(3, "critic", "!support #stage ^1 Bounds the blast radius."),
        said(4, "archivist", "!support #ship ^2 We have shipped this before."),
    ]
}

#[test]
fn a_policy_and_standing_pin_their_wire_forms() {
    let value = serde_json::to_value(QuorumPolicy::DEFAULT).expect("serializes");
    assert_eq!(
        value,
        serde_json::json!({ "threshold": 2, "window": 30, "require_grounded": true }),
    );
    assert_eq!(
        serde_json::from_value::<QuorumPolicy>(value).expect("deserializes"),
        QuorumPolicy::DEFAULT,
    );

    let standing = TopicStanding {
        topic: "stage".into(),
        supporters: vec!["planner".into()],
        silenced: vec!["scout".into()],
        support: 1_400,
    };
    let value = serde_json::to_value(&standing).expect("serializes");
    assert_eq!(
        value,
        serde_json::json!({
            "topic": "stage",
            "supporters": ["planner"],
            "silenced": ["scout"],
            "support": 1_400,
        }),
    );
    assert_eq!(
        serde_json::from_value::<TopicStanding>(value).expect("deserializes"),
        standing,
    );
}

#[test]
fn consensus_pins_its_tagged_wire_form() {
    assert_eq!(
        serde_json::to_value(ConsensusState::Deliberating).expect("serializes"),
        serde_json::json!({ "state": "deliberating" }),
    );
    assert_eq!(
        serde_json::to_value(ConsensusState::Quorum {
            topic: "stage".into()
        })
        .expect("serializes"),
        serde_json::json!({ "state": "quorum", "topic": "stage" }),
    );
    assert_eq!(
        serde_json::to_value(ConsensusState::Deadlocked {
            topics: vec!["stage".into(), "ship".into()],
        })
        .expect("serializes"),
        serde_json::json!({ "state": "deadlocked", "topics": ["stage", "ship"] }),
    );
}

#[test]
fn a_zero_threshold_or_window_is_rejected() {
    let zero_threshold = QuorumPolicy {
        threshold: 0,
        ..QuorumPolicy::DEFAULT
    };
    let error = standings(&[], Sequence(1), &zero_threshold).expect_err("zero threshold");
    assert_eq!(error.to_string(), "quorum threshold must not be zero");

    let zero_window = QuorumPolicy {
        window: 0,
        ..QuorumPolicy::DEFAULT
    };
    let error = standings(&[], Sequence(1), &zero_window).expect_err("zero window");
    assert_eq!(error.to_string(), "quorum window must not be zero");
}

#[test]
fn a_proposer_supports_its_own_topic() {
    let standings = fold(&[said(1, "planner", "!propose #stage")], &policy(2));
    assert_eq!(standing(&standings, "stage").supporters, ["planner"]);
}

#[test]
fn distinct_supporters_carry_a_topic_and_repeat_support_does_not() {
    let repeated = fold(
        &[
            said(1, "planner", "!propose #stage"),
            said(2, "planner", "!support #stage ^1"),
            said(3, "planner", "!support #stage ^1"),
        ],
        &policy(2),
    );
    assert_eq!(standing(&repeated, "stage").supporters, ["planner"]);
    assert_eq!(consensus(&repeated, &policy(2)), ConsensusState::Deliberating);

    let distinct = fold(
        &[
            said(1, "planner", "!propose #stage"),
            said(2, "critic", "!support #stage ^1"),
        ],
        &policy(2),
    );
    assert_eq!(
        consensus(&distinct, &policy(2)),
        ConsensusState::Quorum {
            topic: "stage".into()
        },
    );
}

#[test]
fn an_ungrounded_support_moves_neither_the_supporters_nor_the_weight() {
    let transcript = [
        said(1, "planner", "!propose #stage"),
        said(2, "critic", "!support #stage"),
    ];
    let grounded_required = fold(&transcript, &policy(2));
    let held = standing(&grounded_required, "stage");
    assert_eq!(held.supporters, ["planner"]);
    assert_eq!(held.support, importance(TraceKind::Propose));

    // The same input under a policy that does not require grounds carries.
    let lax = QuorumPolicy {
        require_grounded: false,
        ..policy(2)
    };
    let permissive = fold(&transcript, &lax);
    assert_eq!(standing(&permissive, "stage").supporters, ["planner", "critic"]);
}

#[test]
fn support_outside_the_window_stops_counting() {
    let transcript = [
        said(1, "planner", "!propose #stage"),
        said(50, "critic", "!support #stage ^1"),
    ];
    let narrow = QuorumPolicy {
        window: 10,
        ..policy(2)
    };
    let standings = fold(&transcript, &narrow);
    assert_eq!(standing(&standings, "stage").supporters, ["critic"]);
    assert_eq!(consensus(&standings, &narrow), ConsensusState::Deliberating);
}

#[test]
fn a_trace_without_a_topic_or_an_agent_author_is_not_counted() {
    let transcript = [
        said(1, "planner", "!propose #stage"),
        said(2, "critic", "!support ^1"),
        SessionMessage {
            sequence: Sequence(3),
            author: SessionAuthor::Operator,
            content: "!support #stage ^1".into(),
        },
    ];
    let standings = fold(&transcript, &policy(2));
    assert_eq!(standing(&standings, "stage").supporters, ["planner"]);
}

// --- Cross-inhibition: the mechanism, and the proof that it is load-bearing ---

#[test]
fn two_equally_supported_topics_deadlock() {
    let standings = fold(&deadlocked_transcript(), &policy(2));
    assert_eq!(
        consensus(&standings, &policy(2)),
        ConsensusState::Deadlocked {
            topics: vec!["stage".into(), "ship".into()],
        },
    );
}

#[test]
fn a_grounded_objection_silences_an_advocate_and_breaks_the_deadlock() {
    let mut transcript = deadlocked_transcript();
    // The objection names archivist's supporting message, not the topic.
    transcript.push(said(
        5,
        "critic",
        "!object >4 ^3 That precedent was a different system.",
    ));

    let standings = fold(&transcript, &policy(2));
    let ship = standing(&standings, "ship");
    assert_eq!(ship.silenced, ["archivist"]);
    assert_eq!(ship.supporters, ["scout"]);
    assert_eq!(
        consensus(&standings, &policy(2)),
        ConsensusState::Quorum {
            topic: "stage".into()
        },
        "silencing one advocate must break the tie the same input deadlocks on",
    );
}

#[test]
fn silencing_the_last_advocate_zeroes_a_topics_weight() {
    let transcript = [
        said(1, "planner", "!propose #stage"),
        said(2, "critic", "!object >1 ^0 Not this."),
    ];
    let standings = fold(&transcript, &policy(2));
    let held = standing(&standings, "stage");
    assert!(held.supporters.is_empty());
    assert_eq!(held.silenced, ["planner"]);
    assert_eq!(held.support, 0);
}

#[test]
fn an_objection_cannot_silence_its_own_author() {
    // Otherwise an agent could retract a peer's support by objecting to itself.
    let transcript = [
        said(1, "planner", "!propose #stage"),
        said(2, "planner", "!object >1 ^1 Second thoughts."),
    ];
    assert_eq!(standing(&fold(&transcript, &policy(2)), "stage").supporters, ["planner"]);
}

#[test]
fn an_ungrounded_objection_silences_nobody() {
    let transcript = [
        said(1, "planner", "!propose #stage"),
        said(2, "critic", "!object >1"),
    ];
    let standings = fold(&transcript, &policy(2));
    assert!(standing(&standings, "stage").silenced.is_empty());
}

#[test]
fn an_objection_at_an_unknown_or_absent_target_silences_nobody() {
    let transcript = [
        said(1, "planner", "!propose #stage"),
        said(2, "critic", "!object >99 ^1"),
        said(3, "critic", "!object ^1"),
    ];
    assert!(standing(&fold(&transcript, &policy(2)), "stage").silenced.is_empty());
}

#[test]
fn a_repeated_objection_records_an_advocate_once() {
    let transcript = [
        said(1, "planner", "!propose #stage"),
        said(2, "critic", "!object >1 ^0"),
        said(3, "scout", "!object >1 ^0"),
    ];
    assert_eq!(standing(&fold(&transcript, &policy(2)), "stage").silenced, ["planner"]);
}

// --- Fold discipline ---

#[test]
fn standings_are_order_independent() {
    let transcript = {
        let mut transcript = deadlocked_transcript();
        transcript.push(said(5, "critic", "!object >4 ^3"));
        transcript
    };
    let at = Sequence(5);
    let ordered = read(&transcript);

    let mut shuffled = ordered.clone();
    shuffled.reverse();
    let mut rotated = ordered.clone();
    rotated.rotate_left(2);

    let expected = standings(&ordered, at, &policy(2)).expect("folds");
    for permutation in [shuffled, rotated] {
        assert_eq!(
            standings(&permutation, at, &policy(2)).expect("folds"),
            expected,
            "a reordered fold must land in the same place, topic order included",
        );
    }
}

#[test]
fn standings_are_idempotent_over_duplicated_traces() {
    let transcript = deadlocked_transcript();
    let at = Sequence(4);
    let once = read(&transcript);
    let twice: Vec<_> = once.iter().cloned().chain(once.iter().cloned()).collect();
    assert_eq!(
        standings(&twice, at, &policy(2)).expect("folds"),
        standings(&once, at, &policy(2)).expect("folds"),
    );
}

#[test]
fn an_empty_medium_is_deliberating() {
    let standings = standings(&[], Sequence(0), &policy(2)).expect("folds");
    assert!(standings.is_empty());
    assert_eq!(consensus(&standings, &policy(2)), ConsensusState::Deliberating);
}

#[test]
fn carried_reports_whether_a_standing_reached_the_threshold() {
    let standing = TopicStanding {
        topic: "stage".into(),
        supporters: vec!["a".into(), "b".into()],
        silenced: Vec::new(),
        support: 1,
    };
    assert!(standing.carried(&policy(2)));
    assert!(!standing.carried(&policy(3)));
}
