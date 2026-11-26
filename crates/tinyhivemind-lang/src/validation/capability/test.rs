//! The walker itself: a field with no capability decision is rejected,
//! never passed.
use super::{Judge, Out};
use crate::{HarnessKind, Manifest};
use serde_json::json;

#[test]
fn a_field_with_no_capability_decision_is_rejected() {
    let manifest = Manifest::default();
    for kind in HarnessKind::ALL {
        let mut out = Out::default();
        let mut judge = Judge {
            m: &manifest,
            kind,
            found: &mut out,
        };
        judge.walk(
            "seat",
            &json!({"id": "a", "brand_new": 1}),
            &json!({}),
            "/seats/0",
            "seats.a",
        );
        judge.walk(
            "ghost_owner",
            &json!({"x": true}),
            &json!({}),
            "/ghost",
            "ghost",
        );
        let at: Vec<&str> = out.findings.iter().map(|(_, at)| at.as_str()).collect();
        assert_eq!(at, ["/seats/0/brand_new", "/ghost/x"], "{kind}");
    }
}
