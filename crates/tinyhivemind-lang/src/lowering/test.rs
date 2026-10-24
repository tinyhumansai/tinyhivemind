//! Activation snapshots preserve task, organization and shared context data.

#![allow(clippy::unwrap_used)]

use crate::{Goal, Link, Mission, Package, Role, lower};
use tinyhivemind_core::desk::{Desk, ResponderMode};

#[test]
fn lowering_preserves_host_activation_goals_missions_graph_and_declared_context() {
    let mut package = Package::default();
    package.manifest.id = "project-room".into();
    package.manifest.name = "Project room".into();
    package.manifest.roles.push(Role {
        id: "lead".into(),
        charter: "roles/lead.md".into(),
    });
    package
        .documents
        .insert("roles/lead.md".into(), "Lead the task".into());
    package.manifest.goals = vec![
        Goal {
            id: "deliver".into(),
            description: "Deliver the implementation".into(),
            acceptance: vec!["Reviewed implementation".into()],
            subgoals: vec!["review".into()],
        },
        Goal {
            id: "review".into(),
            description: "Review the change".into(),
            acceptance: vec!["No unresolved feedback".into()],
            subgoals: vec![],
        },
    ];
    package.manifest.missions.push(Mission {
        id: "lead-delivery".into(),
        role: "lead".into(),
        goals: vec!["deliver".into()],
    });
    package.manifest.desks = ["delivery", "review"]
        .into_iter()
        .map(|id| Desk {
            id: id.into(),
            name: id.into(),
            description: None,
            members: vec![],
            responder_mode: ResponderMode::Lead,
        })
        .collect();
    package.manifest.topology = vec![
        Link {
            from: "delivery".into(),
            to: "review".into(),
        },
        Link {
            from: "review".into(),
            to: "delivery".into(),
        },
    ];
    package.manifest.referrals = vec![Link {
        from: "delivery".into(),
        to: "review".into(),
    }];
    package.manifest.context.push("context/task.md".into());
    package.documents.insert(
        "context/task.md".into(),
        "Task constraints\n<!-- item:acceptance -->\nPreserve the host contract\n".into(),
    );
    package.documents.insert(
        "context/unreferenced.md".into(),
        "Host side document".into(),
    );
    let lowered = lower(&package).unwrap();
    assert_eq!(lowered.id, package.manifest.id);
    assert_eq!(lowered.name, package.manifest.name);
    assert_eq!(lowered.goals, package.manifest.goals);
    assert_eq!(lowered.missions, package.manifest.missions);
    assert_eq!(lowered.topology, package.manifest.topology);
    assert_eq!(lowered.referrals, package.manifest.referrals);
    assert_eq!(lowered.context.len(), 1);
    assert_eq!(
        lowered.context["context/task.md"],
        package.documents["context/task.md"]
    );
}

fn integrations() -> Package {
    crate::parse(&crate::validation::test_documents()).unwrap()
}

fn ids<T>(items: &[T], id: impl Fn(&T) -> &str) -> Vec<&str> {
    items.iter().map(id).collect()
}

#[test]
fn a_seat_resolves_its_profile_toolpacks_model_and_hooks() {
    let hive = lower(&integrations()).unwrap();
    let lead = &hive.seats[0].integration;
    // Runtime model and provider, since neither the seat nor its profile
    // names a model.
    let model = lead.model.as_ref().unwrap();
    assert_eq!(model.model.id, "careful");
    assert_eq!(model.provider.as_ref().unwrap().id, "openrouter");
    assert_eq!(lead.profile.as_deref(), Some("editor"));
    // The runtime's `repo` toolpack expands into its tools, server and groups.
    assert_eq!(ids(&lead.programs, |t| &t.id), ["grep_repo", "word_count"]);
    assert_eq!(ids(&lead.mcp, |s| &s.id), ["issues"]);
    assert_eq!(lead.groups["web3"], crate::GroupMode::Off);
    assert_eq!(ids(&lead.skills, |s| &s.id), ["release-notes"]);
    assert_eq!(lead.hooks.len(), 1);
    assert_eq!(lead.hooks[0].tool.id, "guard_shell");
    assert_eq!(lead.subagents, ["summariser"]);
    assert_eq!(lead.limits.timeout_ms, Some(120_000));
    assert_eq!(lead.permissions.len(), 0);
}

#[test]
fn the_most_specific_setting_wins_and_permissions_layer() {
    let hive = lower(&integrations()).unwrap();
    let scout = &hive.seats[1].integration;
    assert_eq!(scout.model.as_ref().unwrap().model.id, "fast");
    // Seat iterations beat the model's; timeout and budget come from the
    // runtime.
    assert_eq!(scout.limits.iterations, Some(6));
    assert_eq!(scout.limits.timeout_ms, Some(120_000));
    assert_eq!(scout.limits.budget_usd, Some(0.5));
    // Profile and seat both name `read_only`; both layers apply.
    assert_eq!(
        ids(&scout.permissions, |p| &p.id),
        ["read_only", "read_only"]
    );
    // Denials concatenate: the profile's then the seat's.
    assert_eq!(scout.tools.deny[0], "write_file");
    assert!(scout.tools.deny.iter().any(|tool| tool == "web_search"));
    assert_eq!(scout.hooks.len(), 0);
    let hive_wide = &hive.integrations;
    assert_eq!(hive_wide.runtime.concurrency, 2);
    assert_eq!(hive_wide.jev.top_k, 5);
    assert_eq!(hive_wide.workflows[0].id, "nightly-digest");
    assert_eq!(hive_wide.model.as_ref().unwrap().model.id, "careful");
    assert_eq!(ids(&hive_wide.skills, |s| &s.id), ["release-notes"]);
}

#[test]
fn a_feature_switched_off_removes_its_integration() {
    let mut package = integrations();
    for name in ["mcp", "skills", "script_tools", "hooks"] {
        package.manifest.features.insert(name.into(), false);
    }
    let hive = lower(&package).unwrap();
    for seat in &hive.seats {
        let integration = &seat.integration;
        assert_eq!(integration.mcp.len(), 0);
        assert_eq!(integration.skills.len(), 0);
        assert_eq!(integration.programs.len(), 0);
        assert_eq!(integration.hooks.len(), 0);
    }
    assert_eq!(hive.integrations.skills.len(), 0);
}

#[test]
fn seat_context_documents_lower_with_their_contents() {
    let mut package = integrations();
    package.manifest.seats[0].context = vec!["context/style.md".into()];
    let hive = lower(&package).unwrap();
    let context = &hive.seats[0].integration.context;
    assert_eq!(context.len(), 1);
    assert_eq!(context[0].0, "context/style.md");
    assert!(context[0].1.contains("imperative"));
}
