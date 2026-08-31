//! Unit tests for the recency, importance and relevance fold.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

use crate::trace::{TopicId, Trace};
use tinyteams::SessionAuthor;

fn trace(sequence: u64, kind: TraceKind) -> Trace {
    Trace {
        sequence: Sequence(sequence),
        author: SessionAuthor::Agent {
            id: "planner".into(),
            label: "planner".into(),
        },
        kind,
        topic: Some(TopicId("stage".into())),
        target: None,
        cites: Vec::new(),
        text: String::new(),
        offset: 0,
    }
}

fn weights(half_life: u32) -> SalienceWeights {
    SalienceWeights {
        half_life,
        ..SalienceWeights::DEFAULT
    }
}

#[test]
fn the_default_weights_are_the_shipped_ones_not_the_published_ones() {
    // The implementation this score is taken from shipped 0.5 / 3.0 / 2.0,
    // which is not what its paper documents. Pinned so a future reader does not
    // "correct" them back to the paper.
    assert_eq!(
        SalienceWeights::default(),
        SalienceWeights {
            recency: 5,
            importance: 30,
            relevance: 20,
            half_life: 20,
        },
    );
}

#[test]
fn the_weights_pin_their_wire_form() {
    let value = serde_json::to_value(SalienceWeights::DEFAULT).expect("serializes");
    assert_eq!(
        value,
        serde_json::json!({
            "recency": 5,
            "importance": 30,
            "relevance": 20,
            "half_life": 20,
        }),
    );
    assert_eq!(
        serde_json::from_value::<SalienceWeights>(value).expect("deserializes"),
        SalienceWeights::DEFAULT,
    );
}

#[test]
fn a_zero_half_life_is_rejected() {
    let error = salience(&trace(1, TraceKind::Propose), Sequence(1), &weights(0), 50)
        .expect_err("zero half life");
    assert_eq!(error.to_string(), "salience half life must not be zero");
}

#[test]
fn recency_halves_once_per_half_life() {
    // Recency alone, so the halving is directly observable.
    let only_recency = SalienceWeights {
        recency: 10,
        importance: 0,
        relevance: 0,
        half_life: 10,
    };
    let at = Sequence(100);
    let score = |sequence| {
        salience(&trace(sequence, TraceKind::Propose), at, &only_recency, 0)
            .expect("scores")
            .0
    };
    assert_eq!(score(100), 1_000);
    assert_eq!(score(90), 500);
    assert_eq!(score(80), 250);
    assert_eq!(score(70), 125);
}

#[test]
fn recency_interpolates_linearly_within_a_half_life() {
    let only_recency = SalienceWeights {
        recency: 10,
        importance: 0,
        relevance: 0,
        half_life: 10,
    };
    let halfway = salience(
        &trace(95, TraceKind::Propose),
        Sequence(100),
        &only_recency,
        0,
    )
    .expect("scores");
    assert_eq!(halfway.0, 750);
}

#[test]
fn a_very_old_trace_decays_to_nothing_rather_than_overflowing() {
    let only_recency = SalienceWeights {
        recency: 10,
        importance: 0,
        relevance: 0,
        half_life: 1,
    };
    let score = salience(
        &trace(0, TraceKind::Propose),
        Sequence(u64::MAX),
        &only_recency,
        0,
    )
    .expect("scores");
    assert_eq!(score.0, 0);
}

#[test]
fn a_trace_newer_than_the_decision_point_does_not_decay() {
    let only_recency = SalienceWeights {
        recency: 10,
        importance: 0,
        relevance: 0,
        half_life: 10,
    };
    let score = salience(
        &trace(50, TraceKind::Propose),
        Sequence(10),
        &only_recency,
        0,
    )
    .expect("scores");
    assert_eq!(score.0, 1_000);
}

#[test]
fn importance_orders_the_trace_kinds() {
    assert!(importance(TraceKind::Commit) > importance(TraceKind::Propose));
    assert!(importance(TraceKind::Propose) > importance(TraceKind::Object));
    assert!(importance(TraceKind::Object) > importance(TraceKind::Evidence));
    assert!(importance(TraceKind::Evidence) > importance(TraceKind::Support));
    assert!(importance(TraceKind::Support) > importance(TraceKind::Question));
}

#[test]
fn a_more_important_kind_outscores_a_less_important_one_at_equal_age() {
    let at = Sequence(10);
    let commit = salience(&trace(10, TraceKind::Commit), at, &weights(20), 50).expect("scores");
    let question = salience(&trace(10, TraceKind::Question), at, &weights(20), 50).expect("scores");
    assert!(commit > question);
}

#[test]
fn relevance_saturates_at_one_hundred() {
    let at = Sequence(1);
    let capped = salience(&trace(1, TraceKind::Support), at, &weights(20), 100).expect("scores");
    let over = salience(&trace(1, TraceKind::Support), at, &weights(20), 255).expect("scores");
    assert_eq!(capped, over);
}

#[test]
fn relevance_moves_the_score_monotonically() {
    let at = Sequence(1);
    let low = salience(&trace(1, TraceKind::Support), at, &weights(20), 0).expect("scores");
    let high = salience(&trace(1, TraceKind::Support), at, &weights(20), 100).expect("scores");
    assert!(high > low);
}

#[test]
fn a_score_pins_its_transparent_wire_form() {
    assert_eq!(
        serde_json::to_value(Salience(1_234)).expect("serializes"),
        serde_json::json!(1_234),
    );
}
