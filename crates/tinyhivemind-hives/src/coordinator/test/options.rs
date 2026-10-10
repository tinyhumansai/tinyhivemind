//! Scheduling options preserve defaults and their complete wire form.
use super::*;

#[test]
fn coordinator_options_default_missing_fields_and_pin_wire_shape() {
    let options: CoordinatorOptions = serde_json::from_str("{}").unwrap();
    let default = CoordinatorOptions::default();
    assert_eq!(
        serde_json::to_value(&options).unwrap(),
        serde_json::to_value(&default).unwrap()
    );
    let wire = serde_json::to_value(&default).unwrap();
    assert_eq!(wire["round_width"], 1);
    assert_eq!(
        wire["conduct_policy"],
        serde_json::json!({"child_turn_wall":6,"turn_wall":60})
    );
    assert_eq!(wire["broadcast_budget"], serde_json::Value::Null);
    assert_eq!(
        wire["retention"],
        serde_json::json!({"settled_episodes":null,"delivered":null,"interrupted":null,"pending_per_agent":null})
    );
    let configured =
        serde_json::json!({"round_width":3,"broadcast_budget":7,"retention":{"delivered":12}});
    let parsed: CoordinatorOptions = serde_json::from_value(configured).unwrap();
    assert_eq!(parsed.round_width, 3);
    assert_eq!(parsed.broadcast_budget, Some(7));
    assert_eq!(parsed.retention.delivered, Some(12));
    assert!(
        serde_json::from_value::<CoordinatorOptions>(serde_json::json!({"surprise":true})).is_err()
    );
    assert!(
        serde_json::from_value::<CoordinatorOptions>(
            serde_json::json!({"retention":{"surprise":true}})
        )
        .is_err()
    );
}
