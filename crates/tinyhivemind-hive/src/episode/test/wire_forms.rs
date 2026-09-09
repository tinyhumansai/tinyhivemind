//! Serde wire-form pins for `EpisodePolicy`, `EpisodeState` and the tagged
//! `HiveStep` variants, plus the shipping default's budget.

use super::super::*;
use super::support::state;

#[test]
fn the_policy_and_state_pin_their_wire_forms() {
    let value = serde_json::to_value(EpisodePolicy::DEFAULT).expect("serializes");
    assert_eq!(
        value,
        serde_json::json!({
            "turn_budget": 12,
            "round_width": 4,
            "revealed_width": 1,
            "blind_round": true,
            "dominance_cap": 50,
            "repetition_cap": 3,
            "directory": null,
            "defer_cap": null,
            "quorum": {
                "threshold": 2,
                "window": 30,
                "require_grounded": true,
                "refutation_cap": null,
                "require_evidential": false,
            },
            "weights": { "recency": 5, "importance": 30, "relevance": 20, "half_life": 20 },
        }),
    );
    assert_eq!(
        serde_json::from_value::<EpisodePolicy>(value).expect("deserializes"),
        EpisodePolicy::DEFAULT,
    );

    let value = serde_json::to_value(state()).expect("serializes");
    assert_eq!(
        value,
        serde_json::json!({
            "conversation": {
                "desk_id": "engineering",
                "desk_name": "Engineering",
                "thread_root": null,
            },
            "spent": 0,
            "phase": "deliberate",
            "thresholds": [],
            "watermark": 0,
            "commit_boundary": null,
        }),
    );
    assert_eq!(
        serde_json::from_value::<EpisodeState>(value).expect("deserializes"),
        state(),
    );
}

#[test]
fn the_default_policy_bounds_the_episode() {
    // A finite budget is what makes termination a property rather than a hope.
    assert_eq!(EpisodePolicy::default(), EpisodePolicy::DEFAULT);
    assert_eq!(EpisodePolicy::default().turn_budget, 12);
}

#[test]
fn every_step_pins_its_tagged_wire_form() {
    assert_eq!(
        serde_json::to_value(HiveStep::Idle).expect("serializes"),
        serde_json::json!({ "step": "idle" }),
    );
    assert_eq!(
        serde_json::to_value(HiveStep::Exhausted { spent: 12 }).expect("serializes"),
        serde_json::json!({ "step": "exhausted", "spent": 12 }),
    );
    assert_eq!(
        serde_json::to_value(HiveStep::Deadlocked {
            topics: vec!["stage".into(), "ship".into()],
        })
        .expect("serializes"),
        serde_json::json!({ "step": "deadlocked", "topics": ["stage", "ship"] }),
    );
}
