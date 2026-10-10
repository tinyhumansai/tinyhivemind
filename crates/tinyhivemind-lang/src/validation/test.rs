//! Package invariants and portable memory wire regression tests.
#![allow(clippy::unwrap_used)]

use crate::*;
use serde_json::json;

fn fixture() -> Package {
    let mut p = Package::default();
    p.manifest.roles.push(Role {
        id: "builder".into(),
        charter: "roles/builder.md".into(),
    });
    p.manifest.seats.push(Seat {
        id: "alice".into(),
        label: "Alice".into(),
        role: "builder".into(),
        prompt: "seats/alice.md".into(),
        template: "agent".into(),
        config: json!({}),
        model: Some("model".into()),
        tools: ToolAccess::default(),
        mcp: vec![],
        skills: vec![],
    });
    p.documents
        .insert("roles/builder.md".into(), "Build reliably".into());
    p.documents
        .insert("seats/alice.md".into(), "Solve the task".into());
    p.memory.identities.push(MemoryIdentity {
        id: "knowledge".into(),
        namespace: "team:hive/agent:writer".into(),
        kind: MemoryKind::Agent,
        lifecycle: MemoryLifecycle::Hive,
        read_only: false,
        fork_parent: None,
    });
    p.memory.bindings.push(SeatMemory {
        seat: "alice".into(),
        identity: "knowledge".into(),
        reads: vec![MemoryReach {
            at: "team:hive/agent:writer".into(),
            inherit: false,
            descendants: false,
        }],
        recall_at: vec![RecallMoment::SessionStart],
        budget_chars: 1024,
        remember: vec![EntryKind::Note],
    });
    p
}
#[test]
fn parses_lowers_and_preserves_roles_memory_and_wire() {
    let p = fixture();
    let mut docs = p.documents.clone();
    docs.insert(
        "hive.json".into(),
        serde_json::to_string(&p.manifest).unwrap(),
    );
    docs.insert(
        "constitution.json".into(),
        serde_json::to_string(&p.constitution).unwrap(),
    );
    docs.insert(
        "memory.json".into(),
        serde_json::to_string(&p.memory).unwrap(),
    );
    assert_eq!(parse(&docs).unwrap(), p);
    let l = lower(&p).unwrap();
    assert_eq!(l.candidates[0].role.as_deref(), Some("builder"));
    assert_eq!(l.seats[0].memory.as_ref().unwrap().agent_id, "writer");
    assert_eq!(l.seats[0].prompt, "Solve the task");
    assert_eq!(
        serde_json::to_value(&p.memory.bindings[0]).unwrap(),
        json!({"seat":"alice","identity":"knowledge","reads":[{"at":"team:hive/agent:writer","inherit":false,"descendants":false}],"recall_at":["session_start"],"budget_chars":1024,"remember":["note"]})
    );
    assert_eq!(
        serde_json::from_value::<Package>(serde_json::to_value(&p).unwrap()).unwrap(),
        p
    );
}
#[test]
fn reports_missing_documents_and_malformed_json() {
    assert!(matches!(
        parse(&std::collections::BTreeMap::default()),
        Err(Error::MissingDocument { .. })
    ));
    let docs = std::collections::BTreeMap::from([("hive.json".into(), "{".into())]);
    assert!(matches!(parse(&docs), Err(Error::Json { .. })));
    let mut p = fixture();
    p.documents.clear();
    assert!(matches!(validate(&p), Err(Error::MissingDocument { .. })));
}
#[test]
fn rejects_unknown_schema_fields_and_versions() {
    let mut value = serde_json::to_value(Manifest::default()).unwrap();
    value["unknown"] = json!(true);
    assert!(serde_json::from_value::<Manifest>(value).is_err());
    let mut p = fixture();
    p.manifest.version = 2;
    assert_eq!(validate(&p), Err(Error::Schema));
}
#[test]
fn rejects_duplicate_and_empty_identities() {
    let mut p = fixture();
    p.manifest.seats.push(p.manifest.seats[0].clone());
    assert!(matches!(validate(&p), Err(Error::Identity { .. })));
    p = fixture();
    p.manifest.id.clear();
    assert!(matches!(validate(&p), Err(Error::Identity { .. })));
}
#[test]
fn rejects_unresolved_references_and_unsafe_paths() {
    let mut p = fixture();
    p.manifest.seats[0].role = "missing".into();
    assert!(matches!(validate(&p), Err(Error::Reference { .. })));
    p = fixture();
    p.documents.insert("../escape.md".into(), "x".into());
    assert!(matches!(validate(&p), Err(Error::Path { .. })));
}
#[test]
fn rejects_zero_bounds_and_constitution_overruns() {
    let mut p = fixture();
    p.manifest.coordinator.round_width = 0;
    assert!(matches!(validate(&p), Err(Error::Bound { .. })));
    p = fixture();
    p.manifest.policies.episode.round_width = 2;
    assert!(matches!(validate(&p), Err(Error::Bound { .. })));
    p = fixture();
    p.constitution.max_turns = 1;
    assert!(matches!(validate(&p), Err(Error::Bound { .. })));
}
#[test]
fn rejects_goal_cycles_but_allows_topology_cycles() {
    let mut p = fixture();
    p.manifest.goals.push(Goal {
        id: "task".into(),
        subgoals: vec!["task".into()],
        ..Goal::default()
    });
    assert!(matches!(validate(&p), Err(Error::GoalCycle { .. })));
}
#[test]
fn rejects_malformed_root_protected_and_read_only_memory_access() {
    let mut p = fixture();
    p.memory.bindings[0].reads[0].at = "root".into();
    assert!(matches!(validate(&p), Err(Error::Namespace { .. })));
    p = fixture();
    p.constitution
        .protected_namespaces
        .push("team:hive/agent:writer".into());
    assert!(matches!(validate(&p), Err(Error::MemoryAccess { .. })));
    p = fixture();
    p.constitution.read_only_memories.push("knowledge".into());
    assert!(matches!(validate(&p), Err(Error::MemoryAccess { .. })));
    p.memory.identities[0].read_only = true;
    assert!(validate(&p).is_ok());
}
#[test]
fn validates_frontmatter_and_lowers_only_prompt_body() {
    let mut p = fixture();
    p.documents.insert(
        "seats/alice.md".into(),
        "---\n{\"role\":\"builder\"}\n---\nBody".into(),
    );
    assert_eq!(lower(&p).unwrap().seats[0].prompt, "Body");
    p.documents.insert(
        "seats/alice.md".into(),
        "---\n{\"role\":\"wrong\"}\n---\nBody".into(),
    );
    assert!(matches!(validate(&p), Err(Error::Frontmatter { .. })));
}
#[test]
fn sharing_reassignment_and_forking_keep_seat_identity() {
    let mut p = fixture();
    let mut seat = p.manifest.seats[0].clone();
    seat.id = "bob".into();
    p.manifest.seats.push(seat);
    let mut binding = p.memory.bindings[0].clone();
    binding.seat = "bob".into();
    p.memory.bindings.push(binding);
    assert!(validate(&p).is_ok());
    p.memory.identities.push(MemoryIdentity {
        id: "fork".into(),
        namespace: "team:hive/agent:writer/agent:shadow".into(),
        fork_parent: Some("knowledge".into()),
        ..p.memory.identities[0].clone()
    });
    p.memory.bindings[1].identity = "fork".into();
    let lowered = lower(&p).unwrap();
    assert_eq!(lowered.seats[1].id, "bob");
    assert_eq!(lowered.seats[1].memory.as_ref().unwrap().agent_id, "shadow");
}
#[test]
fn policy_wire_preserves_existing_defaults() {
    assert_eq!(
        serde_json::to_value(tinyhivemind_core::hive::DivisionPolicy::default()).unwrap(),
        json!({"round_width":4,"follow_directory":true})
    );
    assert_eq!(
        serde_json::to_value(tinyhivemind_core::driver::ConductPolicy::default()).unwrap(),
        json!({"child_turn_wall":6,"turn_wall":60})
    );
}

#[test]
fn resolves_each_declared_reference_and_allows_cyclic_desks() {
    let mut p = fixture();
    let desk = tinyhivemind_core::desk::Desk {
        id: "main".into(),
        name: "Main".into(),
        description: None,
        members: vec!["alice".into()],
        responder_mode: tinyhivemind_core::desk::ResponderMode::default(),
    };
    p.manifest.desks.push(desk.clone());
    let mut other = desk;
    other.id = "other".into();
    p.manifest.desks.push(other);
    p.manifest.topology = vec![
        Link {
            from: "main".into(),
            to: "other".into(),
        },
        Link {
            from: "other".into(),
            to: "main".into(),
        },
    ];
    assert!(validate(&p).is_ok());
    p.manifest.topology[0].to = "absent".into();
    assert!(matches!(validate(&p), Err(Error::Reference { .. })));
    p = fixture();
    p.manifest.goals.push(Goal {
        id: "main".into(),
        subgoals: vec!["absent".into()],
        ..Goal::default()
    });
    assert!(matches!(validate(&p), Err(Error::Reference { .. })));
    p = fixture();
    p.manifest.missions.push(Mission {
        id: "mission".into(),
        role: "absent".into(),
        goals: vec![],
    });
    assert!(matches!(validate(&p), Err(Error::Reference { .. })));
    p = fixture();
    p.manifest.missions.push(Mission {
        id: "mission".into(),
        role: "builder".into(),
        goals: vec!["absent".into()],
    });
    assert!(matches!(validate(&p), Err(Error::Reference { .. })));
    p = fixture();
    p.memory.bindings[0].identity = "absent".into();
    assert!(matches!(validate(&p), Err(Error::Reference { .. })));
    p = fixture();
    p.memory.identities[0].fork_parent = Some("absent".into());
    assert!(matches!(validate(&p), Err(Error::Reference { .. })));
    p = fixture();
    p.manifest.desks.push(tinyhivemind_core::desk::Desk {
        id: "main".into(),
        name: "Main".into(),
        description: None,
        members: vec!["absent".into()],
        responder_mode: tinyhivemind_core::desk::ResponderMode::default(),
    });
    assert!(matches!(validate(&p), Err(Error::Reference { .. })));
}
#[test]
fn namespace_syntax_matches_engine_and_limits_reaches() {
    for namespace in [
        "root".to_owned(),
        "unknown:x".into(),
        "agent:with.dot".into(),
        format!("agent:{}", "x".repeat(129)),
        std::iter::repeat_n("agent:x", 9)
            .collect::<Vec<_>>()
            .join("/"),
    ] {
        assert!(
            matches!(
                validation::validate_namespace(&namespace),
                Err(Error::Namespace { .. })
            ),
            "{namespace}"
        );
    }
    let mut p = fixture();
    p.memory.bindings[0].reads[0].at = "team:other".into();
    assert!(matches!(validate(&p), Err(Error::MemoryAccess { .. })));
    p = fixture();
    p.constitution.protected_namespaces = vec!["team:hive/agent:grader".into()];
    p.memory.bindings[0].reads = vec![MemoryReach {
        at: "team:hive".into(),
        inherit: false,
        descendants: true,
    }];
    assert!(matches!(validate(&p), Err(Error::MemoryAccess { .. })));
    p = fixture();
    p.memory.identities[0].namespace = "team:other/agent:writer".into();
    assert!(matches!(validate(&p), Err(Error::MemoryAccess { .. })));
    p = fixture();
    p.memory.identities.push(MemoryIdentity {
        id: "badfork".into(),
        namespace: "team:hive/agent:shadow".into(),
        fork_parent: Some("knowledge".into()),
        ..p.memory.identities[0].clone()
    });
    assert!(matches!(validate(&p), Err(Error::MemoryAccess { .. })));
}
#[test]
fn pins_manifest_seat_goal_and_memory_identity_wire() {
    let p = fixture();
    assert_eq!(
        serde_json::to_value(&p.manifest.seats[0]).unwrap(),
        json!({"id":"alice","label":"Alice","role":"builder","prompt":"seats/alice.md","template":"agent","config":{},"model":"model","tools":{"allow":[],"deny":[]},"mcp":[],"skills":[]})
    );
    assert_eq!(
        serde_json::to_value(&p.memory.identities[0]).unwrap(),
        json!({"id":"knowledge","namespace":"team:hive/agent:writer","kind":"agent","lifecycle":"hive","read_only":false,"fork_parent":null})
    );
    assert_eq!(
        serde_json::to_value(Role {
            id: "lead".into(),
            charter: "roles/lead.md".into()
        })
        .unwrap(),
        json!({"id":"lead","charter":"roles/lead.md"})
    );
    assert_eq!(
        serde_json::to_value(Goal {
            id: "task".into(),
            description: "Do it".into(),
            acceptance: vec!["done".into()],
            subgoals: vec![]
        })
        .unwrap(),
        json!({"id":"task","description":"Do it","acceptance":["done"],"subgoals":[]})
    );
    assert_eq!(
        serde_json::to_value(Mission {
            id: "work".into(),
            role: "lead".into(),
            goals: vec!["task".into()]
        })
        .unwrap(),
        json!({"id":"work","role":"lead","goals":["task"]})
    );
    assert_eq!(
        serde_json::to_value(Link {
            from: "a".into(),
            to: "b".into()
        })
        .unwrap(),
        json!({"from":"a","to":"b"})
    );
    assert_eq!(
        serde_json::to_value(MemorySpec::default()).unwrap(),
        json!({"root":"team:hive","identities":[],"bindings":[]})
    );
    for (value, wire) in [
        (RecallMoment::SessionStart, "session_start"),
        (RecallMoment::Rejoin, "rejoin"),
        (RecallMoment::Compaction, "compaction"),
    ] {
        assert_eq!(serde_json::to_value(value).unwrap(), json!(wire));
    }
    for (value, wire) in [
        (EntryKind::Observation, "observation"),
        (EntryKind::FailedAttempt, "failed_attempt"),
        (EntryKind::Outcome, "outcome"),
        (EntryKind::Note, "note"),
    ] {
        assert_eq!(serde_json::to_value(value).unwrap(), json!(wire));
    }
}

#[test]
fn validates_long_goal_chains_without_recursing() {
    let mut p = fixture();
    p.manifest.goals = (0..4000)
        .map(|i| Goal {
            id: format!("g{i}"),
            subgoals: if i == 3999 {
                vec![]
            } else {
                vec![format!("g{}", i + 1)]
            },
            ..Goal::default()
        })
        .collect();
    assert!(validate(&p).is_ok());
    p.manifest.goals[3999].subgoals.push("g0".into());
    assert!(matches!(validate(&p), Err(Error::GoalCycle { .. })));
}
#[test]
fn frontmatter_memory_agrees_with_canonical_binding() {
    let mut p = fixture();
    p.documents.insert(
        "seats/alice.md".into(),
        "---\n{\"memory\":\"knowledge\",\"budget_chars\":1024}\n---\nBody".into(),
    );
    assert!(validate(&p).is_ok());
    p.documents.insert(
        "seats/alice.md".into(),
        "---\n{\"memory\":\"knowledge\",\"budget_chars\":1}\n---\nBody".into(),
    );
    assert!(matches!(validate(&p), Err(Error::Frontmatter { .. })));
}
#[test]
fn rejects_control_windows_and_absolute_document_paths() {
    for path in [
        "/secrets.md",
        "C:/secrets.md",
        "a\\b.md",
        "null\0.md",
        "a//b.md",
        "./a.md",
    ] {
        let mut p = fixture();
        p.documents.insert(path.into(), "x".into());
        assert!(matches!(validate(&p), Err(Error::Path { .. })), "{path}");
    }
}

#[test]
fn parser_rejects_nested_core_unknown_fields_and_unused_unsafe_paths() {
    let p = fixture();
    let mut docs = p.documents.clone();
    let mut manifest = serde_json::to_value(&p.manifest).unwrap();
    manifest["policies"]["episode"]["invented"] = json!(1);
    docs.insert("hive.json".into(), manifest.to_string());
    docs.insert(
        "constitution.json".into(),
        serde_json::to_string(&p.constitution).unwrap(),
    );
    docs.insert(
        "memory.json".into(),
        serde_json::to_string(&p.memory).unwrap(),
    );
    assert!(matches!(parse(&docs), Err(Error::Json { .. })));
    docs.insert("../unused.json".into(), "{}".into());
    assert!(matches!(parse(&docs), Err(Error::Path { .. })));
}

#[test]
fn pins_constitution_coordinator_retention_and_package_sections() {
    let c = Constitution::default();
    assert_eq!(
        serde_json::to_value(c).unwrap(),
        json!({"hard_constraints":[],"max_turns":60,"max_width":8,"permitted_edits":[],"read_only_memories":[],"protected_namespaces":[],"contamination_canaries":[],"evaluation":null,"acceptance":null,"telemetry":null})
    );
    assert_eq!(
        serde_json::to_value(CoordinatorSettings::default()).unwrap(),
        json!({"round_width":1,"conduct_policy":{"child_turn_wall":6,"turn_wall":60},"broadcast_budget":null,"retention":{"settled_episodes":null,"delivered":null,"interrupted":null,"pending_per_agent":null}})
    );
    let p = serde_json::to_value(Package::default()).unwrap();
    assert_eq!(
        p.as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec!["constitution", "documents", "manifest", "memory"]
    );
    assert_eq!(
        p["manifest"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec![
            "$schema",
            "context",
            "coordinator",
            "desks",
            "goals",
            "id",
            "missions",
            "name",
            "policies",
            "referrals",
            "roles",
            "seats",
            "topology",
            "version"
        ]
    );
    assert_eq!(
        p["manifest"]["policies"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec![
            "approval",
            "aside",
            "brevity",
            "conduct",
            "digest",
            "directory",
            "division",
            "episode",
            "exchange",
            "referral",
            "routing"
        ]
    );
    assert_eq!(
        serde_json::to_value(MemoryKind::Pool).unwrap(),
        json!("pool")
    );
    assert_eq!(
        serde_json::to_value(MemoryLifecycle::Run).unwrap(),
        json!("run")
    );
}

#[test]
fn parser_reports_each_mandatory_document_and_lead_mode_compatibility() {
    let p = fixture();
    let mut docs = p.documents.clone();
    docs.insert(
        "hive.json".into(),
        serde_json::to_string(&p.manifest).unwrap(),
    );
    assert!(matches!(parse(&docs), Err(Error::MissingDocument { .. })));
    docs.insert(
        "constitution.json".into(),
        serde_json::to_string(&p.constitution).unwrap(),
    );
    assert!(matches!(parse(&docs), Err(Error::MissingDocument { .. })));
    docs.insert(
        "memory.json".into(),
        serde_json::to_string(&p.memory).unwrap(),
    );
    docs.insert("constitution.json".into(), "false".into());
    assert!(matches!(parse(&docs), Err(Error::Json { .. })));
    docs.insert(
        "constitution.json".into(),
        serde_json::to_string(&p.constitution).unwrap(),
    );
    let mut manifest = serde_json::to_value(&p.manifest).unwrap();
    manifest["desks"] = json!([{"id":"main","name":"Main","description":null,"members":["alice"],"responder_mode":"lead"}]);
    docs.insert("hive.json".into(), manifest.to_string());
    assert!(parse(&docs).is_ok());
    manifest["version"] = json!(2);
    docs.insert("hive.json".into(), manifest.to_string());
    assert_eq!(parse(&docs), Err(Error::Schema));
}
#[test]
fn rejects_all_active_policy_zero_budgets() {
    let mutations: [fn(&mut Package); 13] = [
        |p| p.constitution.max_width = 0,
        |p| p.manifest.policies.episode.dominance_cap = 101,
        |p| p.manifest.policies.division.round_width = 0,
        |p| p.manifest.policies.routing.choice_option_limit = 0,
        |p| p.manifest.coordinator.conduct_policy.child_turn_wall = 0,
        |p| p.manifest.coordinator.broadcast_budget = Some(0),
        |p| p.manifest.policies.digest.input_limit = 0,
        |p| {
            p.manifest.policies.aside.enabled = true;
            p.manifest.policies.aside.max_members = 0;
        },
        |p| p.manifest.policies.episode.quorum.threshold = 0,
        |p| p.manifest.policies.episode.repetition_cap = 0,
        |p| p.manifest.policies.episode.weights.half_life = 0,
        |p| {
            p.manifest.policies.referral.enabled = true;
            p.manifest.policies.referral.max_hops = 0;
        },
        |p| p.manifest.policies.brevity.message_chars = 0,
    ];
    for mutate in mutations {
        let mut p = fixture();
        mutate(&mut p);
        assert!(matches!(validate(&p), Err(Error::Bound { .. })));
    }
    let mut p = fixture();
    p.manifest.policies.exchange.enabled = true;
    p.manifest.policies.exchange.contact_cap = 0;
    assert!(matches!(validate(&p), Err(Error::Bound { .. })));
}
#[test]
fn rejects_malformed_frontmatter_and_incomplete_memory_declarations() {
    for metadata in ["---\n{}", "---\nnot json\n---\nbody", "---\n[]\n---\nbody"] {
        let mut p = fixture();
        p.documents.insert("seats/alice.md".into(), metadata.into());
        assert!(matches!(validate(&p), Err(Error::Frontmatter { .. })));
    }
    let mut p = fixture();
    p.manifest.seats[0].template.clear();
    assert!(matches!(validate(&p), Err(Error::Identity { .. })));
    p = fixture();
    p.memory.root = "root".into();
    assert!(matches!(validate(&p), Err(Error::Namespace { .. })));
    p = fixture();
    p.memory.bindings[0].seat = "absent".into();
    assert!(matches!(validate(&p), Err(Error::Reference { .. })));
    p = fixture();
    p.memory.bindings[0].budget_chars = 0;
    assert!(matches!(validate(&p), Err(Error::Bound { .. })));
    p = fixture();
    p.memory.identities[0].namespace = "team:hive".into();
    assert!(matches!(validate(&p), Err(Error::MemoryAccess { .. })));
}

#[test]
fn inherited_memory_reaches_cannot_widen_to_global_root() {
    let mut p = fixture();
    p.memory.bindings[0].reads[0].inherit = true;
    assert!(matches!(validate(&p), Err(Error::MemoryAccess { .. })));
}

#[test]
fn rejects_duplicate_shared_context_item_identities() {
    let mut p = fixture();
    p.manifest.context.push("context/rules.md".into());
    p.documents.insert(
        "context/rules.md".into(),
        "<!-- item:a -->\nFirst\n<!-- item:a -->\nSecond\n".into(),
    );
    assert!(matches!(validate(&p), Err(Error::Context { .. })));
    p.documents.insert(
        "context/rules.md".into(),
        "<!-- item:a -->\nFirst\n<!-- item:b -->\nSecond\n".into(),
    );
    assert!(validate(&p).is_ok());
}

#[test]
fn role_frontmatter_must_agree_with_its_independent_charter() {
    let mut p = fixture();
    p.documents.insert(
        "roles/builder.md".into(),
        "---\n{\"id\":\"builder\"}\n---\nRole purpose".into(),
    );
    assert!(validate(&p).is_ok());
    assert_eq!(
        lower(&p).unwrap().candidates[0].description.as_deref(),
        Some("Role purpose")
    );
    p.documents.insert(
        "roles/builder.md".into(),
        "---\n{\"id\":\"other\"}\n---\nRole purpose".into(),
    );
    assert!(matches!(validate(&p), Err(Error::Frontmatter { .. })));
}

#[test]
fn declared_document_purposes_cannot_cross_patch_class_boundaries() {
    let mut p = fixture();
    p.documents
        .insert("context/shared.md".into(), "shared".into());
    p.manifest.seats[0].prompt = "context/shared.md".into();
    assert!(matches!(validate(&p), Err(Error::Path { .. })));
    p = fixture();
    p.manifest.roles[0].charter = "seats/alice.md".into();
    assert!(matches!(validate(&p), Err(Error::Path { .. })));
    p = fixture();
    p.manifest.context.push("seats/alice.md".into());
    assert!(matches!(validate(&p), Err(Error::Path { .. })));
}
