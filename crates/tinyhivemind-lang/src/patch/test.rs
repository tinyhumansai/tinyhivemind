//! Atomic proposals, authority boundaries, and wire contracts.
#![allow(clippy::unwrap_used)] // Test assertions intentionally fail on unexpected errors.
use super::*;
use crate::{MemoryIdentity, MemoryKind, MemoryLifecycle};
use serde_json::json;

fn package() -> Package {
    let mut package = Package::default();
    package.constitution.permitted_edits = vec![
        PatchClass::Prompt,
        PatchClass::Context,
        PatchClass::Memory,
        PatchClass::Roster,
        PatchClass::Topology,
        PatchClass::Policy,
        PatchClass::Goal,
    ];
    package
        .documents
        .insert("roles/editor.md".into(), "---\n{}\n---\nOriginal\n".into());
    package.documents.insert(
        "context/notes.md".into(),
        "Introduction\n<!-- item:first -->\nFirst\n<!-- item:second -->\nSecond\n".into(),
    );
    package.memory.identities.push(MemoryIdentity {
        id: "learned".into(),
        namespace: "team:hive/agent:learned".into(),
        kind: MemoryKind::Pool,
        lifecycle: MemoryLifecycle::Hive,
        read_only: false,
        fork_parent: None,
    });
    package
}
fn patch(operation: Operation, class: PatchClass) -> Patch {
    Patch {
        class,
        operations: vec![operation],
    }
}
fn prompt(before: &str, after: &str) -> Patch {
    patch(
        Operation::Prompt {
            path: "roles/editor.md".into(),
            before: before.into(),
            after: after.into(),
        },
        PatchClass::Prompt,
    )
}
fn learning(identity: &str, entry: &str, after: &str) -> Patch {
    patch(
        Operation::Learning {
            identity: identity.into(),
            entry: entry.into(),
            watermark: 12,
            before: None,
            after: Some(after.into()),
        },
        PatchClass::Memory,
    )
}
fn json_edit(pointer: &str, before: Value, after: Value, class: PatchClass) -> Patch {
    patch(
        Operation::Json {
            pointer: pointer.into(),
            before: Some(before),
            after: Some(after),
        },
        class,
    )
}

#[test]
fn prompt_and_context_diff_apply_preserve_neighbors_and_preamble() {
    let original = package();
    let mut proposed = original.clone();
    proposed
        .documents
        .insert("roles/editor.md".into(), "---\n{}\n---\nImproved\n".into());
    let proposal = diff(&original, &proposed, PatchClass::Prompt).unwrap();
    assert_eq!(apply(&original, &proposal).unwrap().candidate, proposed);
    let mut proposed = original.clone();
    proposed.documents.insert("context/notes.md".into(),"Introduction\n<!-- item:first -->\nImproved\n<!-- item:second -->\nSecond\n<!-- item:third -->\nThird\n".into());
    let proposal = diff(&original, &proposed, PatchClass::Context).unwrap();
    assert_eq!(proposal.operations.len(), 2);
    assert_eq!(apply(&original, &proposal).unwrap().candidate, proposed);
    let removal = patch(
        Operation::Context {
            path: "context/notes.md".into(),
            item: "first".into(),
            before: Some("First\n".into()),
            after: None,
        },
        PatchClass::Context,
    );
    assert_eq!(
        apply(&original, &removal).unwrap().candidate.documents["context/notes.md"],
        "Introduction\n<!-- item:second -->\nSecond\n"
    );
}

#[test]
fn policy_json_diff_round_trips_and_stale_edits_are_atomic() {
    let original = package();
    let mut proposed = original.clone();
    proposed.manifest.policies.episode.turn_budget = 2;
    let proposal = diff(&original, &proposed, PatchClass::Policy).unwrap();
    assert_eq!(apply(&original, &proposal).unwrap().candidate, proposed);
    let mut stale = proposal.clone();
    stale.operations.push(Operation::Prompt {
        path: "roles/editor.md".into(),
        before: "wrong".into(),
        after: "new".into(),
    });
    stale.class = PatchClass::Prompt;
    assert!(matches!(
        apply(&original, &stale),
        Err(Error::ClassBoundary(_))
    ));
    assert!(matches!(
        apply(&original, &prompt("wrong", "new")),
        Err(Error::Stale(_))
    ));
    assert_eq!(
        original.documents["roles/editor.md"],
        "---\n{}\n---\nOriginal\n"
    );
}

#[test]
fn rejects_each_guard_failure_without_mutating_original() {
    let original = package();
    let mut forbidden = original.clone();
    forbidden.constitution.permitted_edits.clear();
    assert!(matches!(
        apply(&forbidden, &prompt("Original\n", "new")),
        Err(Error::ForbiddenClass)
    ));
    let mut forged = original.clone();
    forged.constitution.telemetry = json!({"enabled":false});
    assert!(matches!(
        guard(&original, &forged, &prompt("Original\n", "new")),
        Err(Error::Immutable)
    ));
    let mut forged = original.clone();
    forged.manifest.name = "forged".into();
    assert!(matches!(
        guard(&original, &forged, &prompt("Original\n", "new")),
        Err(Error::CandidateMismatch)
    ));
    let mut contaminated = original.clone();
    contaminated.constitution.contamination_canaries = vec!["HELD\nOUT".into()];
    assert!(matches!(
        apply(&contaminated, &prompt("Original\n", "HELD\nOUT")),
        Err(Error::Contamination)
    ));
    assert!(matches!(
        apply(&original, &learning("missing", "entry", "learning")),
        Err(Error::Learning(_))
    ));
    assert!(matches!(
        apply(&original, &learning("learned", "", "learning")),
        Err(Error::Learning(_))
    ));
    let mut readonly = original.clone();
    readonly
        .constitution
        .read_only_memories
        .push("learned".into());
    readonly.memory.identities[0].read_only = true;
    assert!(matches!(
        apply(&readonly, &learning("learned", "entry", "learning")),
        Err(Error::ReadOnly(_))
    ));
    let mut new = readonly.clone();
    new.memory.identities[0].namespace = "team:hive/agent:other".into();
    assert!(matches!(
        diff(&readonly, &new, PatchClass::Memory),
        Err(Error::ReadOnly(_))
    ));
    assert!(matches!(
        apply(
            &original,
            &json_edit(
                "/manifest/policies/episode/turn_budget",
                json!(60),
                json!(0),
                PatchClass::Policy
            )
        ),
        Err(Error::Stale(_) | Error::Validation(_))
    ));
    let before = serde_json::to_value(original.manifest.policies.episode.turn_budget).unwrap();
    assert!(matches!(
        apply(
            &original,
            &json_edit(
                "/manifest/policies/episode/turn_budget",
                before,
                json!(0),
                PatchClass::Policy
            )
        ),
        Err(Error::Validation(_))
    ));
}

#[test]
fn rejects_cross_class_immutable_config_and_invalid_pointers() {
    let original = package();
    assert!(matches!(
        apply(
            &original,
            &json_edit(
                "/constitution/telemetry",
                Value::Null,
                json!(false),
                PatchClass::Policy
            )
        ),
        Err(Error::ClassBoundary(_))
    ));
    assert!(matches!(
        apply(
            &original,
            &json_edit(
                "/manifest/policies/missing/child",
                Value::Null,
                json!(1),
                PatchClass::Policy
            )
        ),
        Err(Error::Pointer(_))
    ));
    assert!(matches!(
        apply(
            &original,
            &json_edit(
                "/manifest/policies/bad~2",
                Value::Null,
                json!(1),
                PatchClass::Policy
            )
        ),
        Err(Error::Pointer(_))
    ));
    let mut value = json!({"list":[1],"a/b":{"~":1}});
    mutate(&mut value, "/a~1b/~0", Some(&json!(1)), Some(&json!(2))).unwrap();
    mutate(&mut value, "/list/1", None, Some(&json!(2))).unwrap();
    mutate(&mut value, "/list/0", Some(&json!(1)), None).unwrap();
    assert_eq!(value, json!({"list":[2],"a/b":{"~":2}}));
    assert!(matches!(
        mutate(&mut value, "/list/01", None, None),
        Err(Error::Pointer(_))
    ));
    assert!(matches!(
        mutate(&mut value, "/list/3", None, None),
        Err(Error::Pointer(_))
    ));
    assert!(matches!(
        mutate(&mut value, "/list/0", None, None),
        Err(Error::Stale(_))
    ));
    assert!(matches!(
        mutate(&mut value, "/list/0/child", None, None),
        Err(Error::Pointer(_))
    ));
}

#[test]
fn stable_items_reject_duplicates_marker_injection_and_lost_preambles() {
    let original = package();
    for (id, after) in [
        ("bad id", "new\n"),
        ("first", "<!-- item:injected -->\nnew\n"),
        ("first", "no newline"),
    ] {
        let proposal = patch(
            Operation::Context {
                path: "context/notes.md".into(),
                item: id.into(),
                before: Some("First\n".into()),
                after: Some(after.into()),
            },
            PatchClass::Context,
        );
        assert!(matches!(apply(&original, &proposal), Err(Error::Item(_))));
    }
    assert!(matches!(
        items::items("<!-- item:x -->\na\n<!-- item:x -->\nb\n"),
        Err(Error::Item(_))
    ));
    assert!(matches!(items::items("<!-- item:x\n"), Err(Error::Item(_))));
    let mut proposed = original.clone();
    proposed.documents.insert(
        "context/notes.md".into(),
        "Deleted preamble\n<!-- item:first -->\nFirst\n<!-- item:second -->\nSecond\n".into(),
    );
    assert!(matches!(
        diff(&original, &proposed, PatchClass::Context),
        Err(Error::CandidateMismatch)
    ));
    proposed = original.clone();
    proposed.documents.insert(
        "roles/editor.md".into(),
        "---\n{\"role\":\"other\"}\n---\nOriginal\n".into(),
    );
    assert!(matches!(
        diff(&original, &proposed, PatchClass::Prompt),
        Err(Error::Immutable)
    ));
}

#[test]
fn learning_operations_leave_package_unchanged_and_pin_wire_form() {
    let original = package();
    let proposal = learning("learned", "entry", "New learning");
    let applied = apply(&original, &proposal).unwrap();
    assert_eq!(applied.candidate, original);
    assert_eq!(applied.learning, proposal.operations);
    let wire = json!({"class":"memory","operations":[{"operation":"learning","identity":"learned","entry":"entry","watermark":12,"before":null,"after":"New learning"}]});
    assert_eq!(serde_json::to_value(&proposal).unwrap(), wire);
    assert_eq!(serde_json::from_value::<Patch>(wire).unwrap(), proposal);
    assert_eq!(
        serde_json::to_value(prompt("a", "b")).unwrap(),
        json!({"class":"prompt","operations":[{"operation":"prompt","path":"roles/editor.md","before":"a","after":"b"}]})
    );
    assert_eq!(
        serde_json::to_value(json_edit(
            "/memory/root",
            json!("a"),
            json!("b"),
            PatchClass::Memory
        ))
        .unwrap(),
        json!({"class":"memory","operations":[{"operation":"json","pointer":"/memory/root","before":"a","after":"b"}]})
    );
    let context = patch(
        Operation::Context {
            path: "context/a.md".into(),
            item: "a".into(),
            before: None,
            after: Some("text\n".into()),
        },
        PatchClass::Context,
    );
    assert_eq!(
        serde_json::to_value(context).unwrap(),
        json!({"class":"context","operations":[{"operation":"context","path":"context/a.md","item":"a","before":null,"after":"text\n"}]})
    );
    for (class, name) in [
        (PatchClass::Prompt, "prompt"),
        (PatchClass::Context, "context"),
        (PatchClass::Memory, "memory"),
        (PatchClass::Roster, "roster"),
        (PatchClass::Topology, "topology"),
        (PatchClass::Policy, "policy"),
        (PatchClass::Goal, "goal"),
    ] {
        assert_eq!(serde_json::to_value(class).unwrap(), json!(name));
        assert_eq!(
            serde_json::from_value::<PatchClass>(json!(name)).unwrap(),
            class
        );
    }
}

#[test]
fn moving_judging_settings_changes_authority_even_when_values_match() {
    let original = json!({"seats":[{"config":{"logging":{"enabled":true}}},{"config":{}}]});
    let moved = json!({"seats":[{"config":{}},{"config":{"logging":{"enabled":true}}}]});
    assert_ne!(protected(&original), protected(&moved));
    let mut original = package();
    original.documents.insert(
        "roles/editor.md".into(),
        "---\n{\"evaluation\":true}\n---\nOriginal\n".into(),
    );
    let mut proposed = original.clone();
    proposed.documents.remove("roles/editor.md");
    proposed.documents.insert(
        "roles/other.md".into(),
        "---\n{\"evaluation\":true}\n---\nOriginal\n".into(),
    );
    assert!(matches!(
        diff(&original, &proposed, PatchClass::Roster),
        Err(Error::Immutable)
    ));
}

#[test]
fn roster_can_create_and_retire_documents_and_policy_can_change_coordinator() {
    let original = package();
    let mut proposed = original.clone();
    proposed.manifest.roles.push(crate::Role {
        id: "new".into(),
        charter: "roles/new.md".into(),
    });
    proposed
        .documents
        .insert("roles/new.md".into(), "New role charter\n".into());
    let proposal = diff(&original, &proposed, PatchClass::Roster).unwrap();
    assert_eq!(apply(&original, &proposal).unwrap().candidate, proposed);
    let retiring = diff(&proposed, &original, PatchClass::Roster).unwrap();
    assert_eq!(apply(&proposed, &retiring).unwrap().candidate, original);
    let mut proposed = original.clone();
    proposed.manifest.coordinator.broadcast_budget = Some(4);
    assert_eq!(
        apply(
            &original,
            &diff(&original, &proposed, PatchClass::Policy).unwrap()
        )
        .unwrap()
        .candidate,
        proposed
    );
    let mut proposed = original.clone();
    proposed.manifest.context.push("context/new.md".into());
    proposed.documents.insert(
        "context/new.md".into(),
        "<!-- item:a -->\nNew context\n".into(),
    );
    assert_eq!(
        apply(
            &original,
            &diff(&original, &proposed, PatchClass::Context).unwrap()
        )
        .unwrap()
        .candidate,
        proposed
    );
    let rewrite = patch(
        Operation::Document {
            path: "context/notes.md".into(),
            before: Some(original.documents["context/notes.md"].clone()),
            after: Some("Collapsed".into()),
        },
        PatchClass::Context,
    );
    assert!(matches!(
        apply(&original, &rewrite),
        Err(Error::ClassBoundary(_))
    ));
    assert_eq!(
        serde_json::to_value(patch(
            Operation::Document {
                path: "roles/new.md".into(),
                before: None,
                after: Some("New role".into())
            },
            PatchClass::Roster
        ))
        .unwrap(),
        json!({"class":"roster","operations":[{"operation":"document","path":"roles/new.md","before":null,"after":"New role"}]})
    );
}

#[test]
fn configured_canary_does_not_contaminate_a_clean_candidate() {
    let mut original = package();
    original.constitution.contamination_canaries = vec!["HELD\nOUT".into()];
    assert!(apply(&original, &prompt("Original\n", "Clean replacement\n")).is_ok());
}

fn seated_package() -> Package {
    use crate::{EntryKind, RecallMoment, SeatMemory};
    let mut original = package();
    original.manifest.roles.push(crate::Role {
        id: "editor".into(),
        charter: "roles/editor.md".into(),
    });
    for id in ["a", "b"] {
        original
            .documents
            .insert(format!("seats/{id}.md"), "Work\n".into());
        original.manifest.seats.push(crate::Seat {
            id: id.into(),
            label: id.into(),
            role: "editor".into(),
            prompt: format!("seats/{id}.md"),
            template: "host".into(),
            config: json!({}),
            ..crate::Seat::default()
        });
        original.memory.identities.push(MemoryIdentity {
            id: id.into(),
            namespace: format!("team:hive/agent:{id}"),
            kind: MemoryKind::Agent,
            lifecycle: MemoryLifecycle::Run,
            read_only: false,
            fork_parent: None,
        });
        original.memory.bindings.push(SeatMemory {
            seat: id.into(),
            identity: id.into(),
            reads: vec![],
            recall_at: vec![RecallMoment::SessionStart],
            budget_chars: 2048,
            remember: vec![EntryKind::Note],
        });
    }
    crate::validate(&original).unwrap();
    original
}

#[test]
fn memory_share_reassign_and_fork_preserve_seat_identities() {
    let original = seated_package();
    let mut shared = original.clone();
    shared.memory.bindings[0].identity = "learned".into();
    shared.memory.bindings[1].identity = "learned".into();
    let patch = diff(&original, &shared, PatchClass::Memory).unwrap();
    assert_eq!(apply(&original, &patch).unwrap().candidate, shared);
    let mut reassigned = shared.clone();
    reassigned.memory.bindings[1].identity = "a".into();
    assert_eq!(
        apply(
            &shared,
            &diff(&shared, &reassigned, PatchClass::Memory).unwrap()
        )
        .unwrap()
        .candidate,
        reassigned
    );
    let mut forked = reassigned.clone();
    forked.memory.identities.push(MemoryIdentity {
        id: "child".into(),
        namespace: "team:hive/agent:a/agent:child".into(),
        kind: MemoryKind::Agent,
        lifecycle: MemoryLifecycle::Hive,
        read_only: false,
        fork_parent: Some("a".into()),
    });
    forked.memory.bindings[1].identity = "child".into();
    assert_eq!(
        apply(
            &reassigned,
            &diff(&reassigned, &forked, PatchClass::Memory).unwrap()
        )
        .unwrap()
        .candidate,
        forked
    );
    assert_eq!(forked.manifest.seats, original.manifest.seats);
    let mut pinned = original.clone();
    pinned.memory.identities[1].read_only = true;
    pinned.constitution.read_only_memories.push("a".into());
    let mut tampered = pinned.clone();
    tampered.memory.bindings[0].identity = "b".into();
    assert!(matches!(
        diff(&pinned, &tampered, PatchClass::Memory),
        Err(Error::ReadOnly(_))
    ));
    let mut protected = original.clone();
    protected.constitution.protected_namespaces = vec!["team:hive/agent:eval".into()];
    let old = serde_json::to_value(&protected.memory.bindings).unwrap();
    let mut proposed = protected.clone();
    proposed.memory.bindings[0].reads.push(crate::MemoryReach {
        at: "team:hive/agent:eval".into(),
        inherit: false,
        descendants: false,
    });
    let proposal = json_edit(
        "/memory/bindings",
        old,
        serde_json::to_value(&proposed.memory.bindings).unwrap(),
        PatchClass::Memory,
    );
    assert!(matches!(
        apply(&protected, &proposal),
        Err(Error::Validation(crate::Error::MemoryAccess { .. }))
    ));
    proposed = original.clone();
    proposed.memory.bindings[0].reads.push(crate::MemoryReach {
        at: "root".into(),
        inherit: false,
        descendants: true,
    });
    let proposal = json_edit(
        "/memory/bindings",
        serde_json::to_value(&original.memory.bindings).unwrap(),
        serde_json::to_value(&proposed.memory.bindings).unwrap(),
        PatchClass::Memory,
    );
    assert!(matches!(
        apply(&original, &proposal),
        Err(Error::Validation(_))
    ));
}

#[test]
fn malformed_typed_json_values_and_prompt_headers_are_rejected() {
    let original = package();
    let proposal = json_edit(
        "/manifest/policies",
        serde_json::to_value(&original.manifest.policies).unwrap(),
        json!("wrong type"),
        PatchClass::Policy,
    );
    assert!(matches!(apply(&original, &proposal), Err(Error::Wire(_))));
    assert!(matches!(
        apply(
            &original,
            &prompt("Original\n", "---\n{\"evaluation\":true}\n---\n")
        ),
        Err(Error::Immutable)
    ));
    let hidden = json!({"seats":[{"config":{"goals":{"acceptance":true}}}]});
    assert_eq!(protected(&hidden).len(), 1);
    let mut readonly = original.clone();
    readonly.memory.identities[0].read_only = true;
    assert!(matches!(
        apply(&readonly, &learning("learned", "entry", "content")),
        Err(Error::ReadOnly(_))
    ));
    let mut rejected = original.clone();
    rejected
        .documents
        .insert("context/notes.md".into(), "No newline".into());
    let proposal = patch(
        Operation::Context {
            path: "context/notes.md".into(),
            item: "new".into(),
            before: None,
            after: Some("new\n".into()),
        },
        PatchClass::Context,
    );
    assert!(matches!(apply(&rejected, &proposal), Err(Error::Item(_))));
}

#[test]
fn removing_and_recreating_a_document_cannot_bypass_item_authority() {
    let original = package();
    let proposal = Patch {
        class: PatchClass::Context,
        operations: vec![
            Operation::Document {
                path: "context/notes.md".into(),
                before: Some(original.documents["context/notes.md"].clone()),
                after: None,
            },
            Operation::Document {
                path: "context/notes.md".into(),
                before: None,
                after: Some("Collapsed context\n".into()),
            },
        ],
    };
    assert!(matches!(
        apply(&original, &proposal),
        Err(Error::ClassBoundary(_))
    ));
}

#[test]
fn unknown_nested_policy_fields_never_silently_disappear() {
    let original = package();
    for field in ["typo", "evaluation"] {
        let proposal = patch(
            Operation::Json {
                pointer: format!("/manifest/policies/episode/{field}"),
                before: None,
                after: Some(json!(true)),
            },
            PatchClass::Policy,
        );
        assert!(matches!(
            apply(&original, &proposal),
            Err(Error::UnknownField)
        ));
    }
}
