//! Evaluated design identities, wire evidence, and archive folds.
#![allow(clippy::unwrap_used)] // Test assertions intentionally fail on unexpected errors.
use super::*;
use crate::patch::{Patch, PatchClass};
use crate::{MemoryIdentity, MemoryKind, MemoryLifecycle};
use serde_json::json;
fn metadata(parent: Option<String>, quality: f64, cost: f64, verdict: Verdict) -> VersionMetadata {
    VersionMetadata {
        parent,
        patch: Patch {
            class: PatchClass::Prompt,
            operations: vec![],
        },
        rationale: "Evaluate held-out tasks".into(),
        transcript_refs: vec![1, 4],
        scores: BTreeMap::from([
            (
                "quality".into(),
                ObjectiveScore {
                    value: quality,
                    direction: Direction::Maximize,
                },
            ),
            (
                "cost".into(),
                ObjectiveScore {
                    value: cost,
                    direction: Direction::Minimize,
                },
            ),
        ]),
        verdict,
        memory_watermarks: BTreeMap::new(),
    }
}
fn design(name: &str, parent: Option<String>, quality: f64, cost: f64, verdict: Verdict) -> Record {
    let mut package = Package::default();
    package.manifest.name = name.into();
    record(&package, metadata(parent, quality, cost, verdict)).unwrap()
}

#[test]
fn exact_candidate_and_memory_watermarks_define_content_identity() {
    let mut package = Package::default();
    let mut evidence = metadata(None, 1.0, 1.0, Verdict::Accepted);
    let initial = record(&package, evidence.clone()).unwrap();
    verify(&initial, &package).unwrap();
    package.manifest.name = "different".into();
    assert!(matches!(verify(&initial, &package), Err(Error::Identity)));
    package.memory.identities.push(MemoryIdentity {
        id: "persistent".into(),
        namespace: "team:hive/agent:learned".into(),
        kind: MemoryKind::Pool,
        lifecycle: MemoryLifecycle::Hive,
        read_only: false,
        fork_parent: None,
    });
    assert!(matches!(
        content_id(&package, &BTreeMap::new()),
        Err(Error::Watermarks)
    ));
    evidence.memory_watermarks.insert("persistent".into(), 12);
    let first = record(&package, evidence.clone()).unwrap();
    evidence.memory_watermarks.insert("persistent".into(), 13);
    let second = record(&package, evidence.clone()).unwrap();
    assert_ne!(first.id, second.id);
    verify(&first, &package).unwrap();
    evidence.memory_watermarks.insert("extra".into(), 1);
    assert!(matches!(record(&package, evidence), Err(Error::Watermarks)));
    let mut malformed = first;
    malformed.class = PatchClass::Goal;
    assert!(matches!(verify(&malformed, &package), Err(Error::Class)));
}

#[test]
fn canonical_objects_ignore_insertion_order_but_preserve_array_order() {
    let mut first = Package::default();
    let mut second = first.clone();
    first.constitution.evaluation = serde_json::from_str(r#"{"b":2,"a":{"z":3,"y":4}}"#).unwrap();
    second.constitution.evaluation = serde_json::from_str(r#"{"a":{"y":4,"z":3},"b":2}"#).unwrap();
    assert_eq!(
        content_id(&first, &BTreeMap::new()).unwrap(),
        content_id(&second, &BTreeMap::new()).unwrap()
    );
    first.constitution.hard_constraints = vec!["a".into(), "b".into()];
    second.constitution.hard_constraints = vec!["b".into(), "a".into()];
    assert_ne!(
        content_id(&first, &BTreeMap::new()).unwrap(),
        content_id(&second, &BTreeMap::new()).unwrap()
    );
}

#[test]
fn rejected_designs_remain_frontier_evidence_and_never_become_heads() {
    let first = design("first", None, 1.0, 3.0, Verdict::Accepted);
    let second = design(
        "second",
        Some(first.id.clone()),
        2.0,
        2.0,
        Verdict::Accepted,
    );
    let rejected = design(
        "rejected",
        Some(second.id.clone()),
        3.0,
        4.0,
        Verdict::Rejected,
    );
    let records = vec![first, second, rejected];
    assert_eq!(accepted_head(&records).unwrap().unwrap().id, records[1].id);
    assert_eq!(
        pareto_frontier(&records)
            .unwrap()
            .iter()
            .map(|record| &record.id)
            .collect::<Vec<_>>(),
        vec![&records[1].id, &records[2].id]
    );
    assert!(accepted_head(&[]).unwrap().is_none());
    assert_eq!(pareto_frontier(&[]).unwrap().len(), 0);
    let rejected = design("rejected-only", None, 1.0, 1.0, Verdict::Rejected);
    assert!(accepted_head(&[rejected]).unwrap().is_none());
}

#[test]
fn rejects_invalid_archive_shapes_and_objective_comparisons() {
    let first = design("first", None, 1.0, 1.0, Verdict::Accepted);
    assert!(matches!(
        accepted_head(&[first.clone(), first.clone()]),
        Err(Error::Duplicate)
    ));
    let mut bad = first.clone();
    bad.parent = Some("missing".into());
    assert!(matches!(accepted_head(&[bad]), Err(Error::Parent)));
    let mut bad = first.clone();
    bad.id = "invalid".into();
    assert!(matches!(accepted_head(&[bad]), Err(Error::Identity)));
    let mut bad = first.clone();
    bad.class = PatchClass::Goal;
    assert!(matches!(accepted_head(&[bad]), Err(Error::Class)));
    let mut bad = first.clone();
    bad.scores.clear();
    assert!(matches!(accepted_head(&[bad]), Err(Error::Scores)));
    let mut bad = first.clone();
    bad.scores.get_mut("quality").unwrap().value = f64::INFINITY;
    assert!(matches!(accepted_head(&[bad]), Err(Error::Scores)));
    let mut second = design(
        "second",
        Some(first.id.clone()),
        1.0,
        1.0,
        Verdict::Rejected,
    );
    assert_eq!(
        pareto_frontier(&[first.clone(), second.clone()])
            .unwrap()
            .len(),
        2
    );
    second.scores.get_mut("quality").unwrap().direction = Direction::Minimize;
    assert!(matches!(
        pareto_frontier(&[first.clone(), second.clone()]),
        Err(Error::Incomparable)
    ));
    second.scores.remove("quality");
    assert!(matches!(
        pareto_frontier(&[first, second]),
        Err(Error::Incomparable)
    ));
    let mut package = Package::default();
    package.manifest.id.clear();
    assert!(matches!(
        record(&package, metadata(None, 1.0, 1.0, Verdict::Accepted)),
        Err(Error::Validation(_))
    ));
}

#[test]
fn record_and_metadata_wire_evidence_round_trip() {
    assert_eq!(
        serde_json::to_value(Verdict::Accepted).unwrap(),
        json!("accepted")
    );
    let package = Package::default();
    let evidence = metadata(None, 0.75, 20.0, Verdict::Rejected);
    let record = record(&package, evidence.clone()).unwrap();
    let wire = json!({"id":record.id,"parent":null,"patch":{"class":"prompt","operations":[]},"class":"prompt","rationale":"Evaluate held-out tasks","transcript_refs":[1,4],"scores":{"quality":{"value":0.75,"direction":"maximize"},"cost":{"value":20.0,"direction":"minimize"}},"verdict":"rejected","memory_watermarks":{}});
    assert_eq!(serde_json::to_value(&record).unwrap(), wire);
    assert_eq!(serde_json::from_value::<Record>(wire).unwrap(), record);
    let mut wire = serde_json::to_value(&evidence).unwrap();
    assert_eq!(wire["verdict"], "rejected");
    assert_eq!(wire["patch"]["class"], "prompt");
    assert_eq!(
        serde_json::from_value::<VersionMetadata>(wire.clone()).unwrap(),
        evidence
    );
    wire["unknown"] = json!(true);
    assert!(serde_json::from_value::<VersionMetadata>(wire).is_err());
}
