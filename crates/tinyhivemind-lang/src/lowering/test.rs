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
