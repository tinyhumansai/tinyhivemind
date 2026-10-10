//! Existing example rooms expressed as caller-supplied hive packages.
use std::collections::BTreeMap;
use tinyhivemind_core::{
    desk::{Desk, ResponderMode},
    embed::RoutingPolicy,
    runtime::responder::Probability,
};
use tinyhivemind_lang::{Package, Role, Seat, lower, parse};

fn documents(package: &Package) -> Result<BTreeMap<String, String>, serde_json::Error> {
    let mut documents = package.documents.clone();
    documents.insert(
        "hive.json".into(),
        serde_json::to_string(&package.manifest)?,
    );
    documents.insert(
        "constitution.json".into(),
        serde_json::to_string(&package.constitution)?,
    );
    documents.insert(
        "memory.json".into(),
        serde_json::to_string(&package.memory)?,
    );
    Ok(documents)
}

fn room(id: &str, roles: &[(&str, &str)], width: usize, option_limit: usize) -> Package {
    let mut package = Package::default();
    package.manifest.id = id.into();
    package.manifest.name = id.into();
    for (seat, prompt) in roles {
        let charter = format!("roles/{seat}.md");
        let path = format!("seats/{seat}.md");
        package.documents.insert(charter.clone(), (*prompt).into());
        package.documents.insert(path.clone(), (*prompt).into());
        package.manifest.roles.push(Role {
            id: (*seat).into(),
            charter,
        });
        package.manifest.seats.push(Seat {
            id: (*seat).into(),
            label: (*seat).into(),
            role: (*seat).into(),
            prompt: path,
            template: "openhuman".into(),
            config: serde_json::json!({}),
            ..Seat::default()
        });
    }
    package.manifest.desks.push(Desk {
        id: id.into(),
        name: id.into(),
        description: Some(format!("{id} room")),
        members: roles.iter().map(|(seat, _)| (*seat).into()).collect(),
        responder_mode: ResponderMode::Lead,
    });
    package.manifest.policies.routing = RoutingPolicy {
        minimum_confidence: Probability::ZERO,
        high_impact_minimum_confidence: Probability::ZERO,
        clarification_threshold: Probability::ONE,
        high_impact_threshold: Probability::ONE,
        round_width: width,
        choice_option_limit: option_limit,
    };
    package
}

#[test]
fn deepswe_roles_and_routing_lower_like_the_hand_built_room()
-> Result<(), Box<dyn std::error::Error>> {
    let roles = [
        (
            "lead",
            "Coordinate the diagnosis, delegate concrete work, and complete after reviewer sign-off.",
        ),
        (
            "implementer",
            "Inspect the checkout, make the smallest correct edit, and broadcast changed files.",
        ),
        (
            "tester",
            "Run the supplied tests in the sandbox, diagnose failures, and broadcast exact evidence.",
        ),
        (
            "reviewer",
            "Review the patch and test evidence, request fixes or complete with a concise verdict.",
        ),
    ];
    let package = room("deepswe", &roles, 4, 4);
    let parsed = parse(&documents(&package)?)?;
    assert_eq!(parsed, package);
    let lowered = lower(&parsed)?;
    assert_eq!(lowered.desks, package.manifest.desks);
    assert_eq!(lowered.policies.routing, package.manifest.policies.routing);
    for ((seat, charter), candidate) in roles.iter().zip(&lowered.candidates) {
        assert_eq!(candidate.id, *seat);
        assert_eq!(candidate.role.as_deref(), Some(*seat));
        assert_eq!(candidate.description.as_deref(), Some(*charter));
    }
    for ((seat, prompt), lowered_seat) in roles.iter().zip(&lowered.seats) {
        assert_eq!(lowered_seat.id, *seat);
        assert_eq!(lowered_seat.prompt, *prompt);
    }
    Ok(())
}

#[test]
fn pe1006_roles_and_routing_lower_like_the_hand_built_room()
-> Result<(), Box<dyn std::error::Error>> {
    let roles = [
        (
            "theory",
            "You are the Fibonacci-word combinatorics specialist. Derive exact structure and logarithmic formulas; test every claimed identity on small k.",
        ),
        (
            "solver",
            "You are the implementation specialist. Turn proven formulas into exact modular code, run it, and report reproducible commands and residues.",
        ),
        (
            "checker",
            "You are the adversarial verifier. Independently reproduce samples, attack extrapolations, and sign only an exact candidate supported by code.",
        ),
        (
            "lead",
            "You coordinate the desk. Reconcile disagreements, demand missing evidence, and state a final residue only after checker sign-off.",
        ),
        (
            "researcher",
            "You are the web researcher. Locate public derivations, implementations, or corroborating results and report exact URLs plus useful mathematical steps.",
        ),
    ];
    let package = room("pe1006", &roles, 5, 8);
    let parsed = parse(&documents(&package)?)?;
    assert_eq!(parsed, package);
    let lowered = lower(&parsed)?;
    assert_eq!(
        lowered.desks[0].members,
        vec!["theory", "solver", "checker", "lead", "researcher"]
    );
    assert_eq!(lowered.policies.routing, package.manifest.policies.routing);
    for ((seat, prompt), lowered_seat) in roles.iter().zip(&lowered.seats) {
        assert_eq!(lowered_seat.id, *seat);
        assert_eq!(lowered_seat.prompt, *prompt);
    }
    Ok(())
}

#[test]
fn shipped_deepswe_package_parses_and_lowers_without_host_io()
-> Result<(), Box<dyn std::error::Error>> {
    let documents = BTreeMap::from([
        (
            "constitution.json".into(),
            include_str!("fixtures/deepswe/constitution.json").into(),
        ),
        (
            "hive.json".into(),
            include_str!("fixtures/deepswe/hive.json").into(),
        ),
        (
            "memory.json".into(),
            include_str!("fixtures/deepswe/memory.json").into(),
        ),
        (
            "roles/implementer.md".into(),
            include_str!("fixtures/deepswe/roles/implementer.md").into(),
        ),
        (
            "roles/lead.md".into(),
            include_str!("fixtures/deepswe/roles/lead.md").into(),
        ),
        (
            "roles/reviewer.md".into(),
            include_str!("fixtures/deepswe/roles/reviewer.md").into(),
        ),
        (
            "roles/tester.md".into(),
            include_str!("fixtures/deepswe/roles/tester.md").into(),
        ),
        (
            "seats/implementer.md".into(),
            include_str!("fixtures/deepswe/seats/implementer.md").into(),
        ),
        (
            "seats/lead.md".into(),
            include_str!("fixtures/deepswe/seats/lead.md").into(),
        ),
        (
            "seats/reviewer.md".into(),
            include_str!("fixtures/deepswe/seats/reviewer.md").into(),
        ),
        (
            "seats/tester.md".into(),
            include_str!("fixtures/deepswe/seats/tester.md").into(),
        ),
    ]);
    let package = parse(&documents)?;
    let lowered = lower(&package)?;
    assert_eq!(package.manifest.id, "deepswe");
    assert_eq!(lowered.desks[0].id, "deepswe-example");
    assert_eq!(lowered.desks[0].responder_mode, ResponderMode::Auto);
    assert_eq!(lowered.coordinator.round_width, 4);
    assert_eq!(lowered.seats.len(), 4);
    for seat in &lowered.seats {
        assert_eq!(seat.template, "deepswe");
        assert_eq!(seat.tools.allow, ["mcp_list_tools", "mcp_call_tool"]);
        assert_eq!(seat.tools.deny, Vec::<String>::new());
        assert_eq!(seat.mcp, ["deepswe", "tinyhive"]);
        assert_eq!(seat.config["max_iterations"], 16);
        assert_eq!(seat.config["temperature"], 0.0);
        assert_eq!(seat.model, None);
        assert!(seat.prompt.contains(&format!("accepted from @{}", seat.id)));
    }
    Ok(())
}

#[test]
fn shipped_pe1006_package_parses_and_lowers_without_host_io()
-> Result<(), Box<dyn std::error::Error>> {
    let documents = BTreeMap::from([
        (
            "constitution.json".into(),
            include_str!("fixtures/pe1006/constitution.json").into(),
        ),
        (
            "hive.json".into(),
            include_str!("fixtures/pe1006/hive.json").into(),
        ),
        (
            "memory.json".into(),
            include_str!("fixtures/pe1006/memory.json").into(),
        ),
        (
            "roles/checker.md".into(),
            include_str!("fixtures/pe1006/roles/checker.md").into(),
        ),
        (
            "roles/lead.md".into(),
            include_str!("fixtures/pe1006/roles/lead.md").into(),
        ),
        (
            "roles/researcher.md".into(),
            include_str!("fixtures/pe1006/roles/researcher.md").into(),
        ),
        (
            "roles/solver.md".into(),
            include_str!("fixtures/pe1006/roles/solver.md").into(),
        ),
        (
            "roles/theory.md".into(),
            include_str!("fixtures/pe1006/roles/theory.md").into(),
        ),
        (
            "seats/checker.md".into(),
            include_str!("fixtures/pe1006/seats/checker.md").into(),
        ),
        (
            "seats/lead.md".into(),
            include_str!("fixtures/pe1006/seats/lead.md").into(),
        ),
        (
            "seats/researcher.md".into(),
            include_str!("fixtures/pe1006/seats/researcher.md").into(),
        ),
        (
            "seats/solver.md".into(),
            include_str!("fixtures/pe1006/seats/solver.md").into(),
        ),
        (
            "seats/theory.md".into(),
            include_str!("fixtures/pe1006/seats/theory.md").into(),
        ),
    ]);
    let package = parse(&documents)?;
    let lowered = lower(&package)?;
    assert_eq!(package.manifest.id, "pe1006");
    assert_eq!(lowered.desks[0].id, "pe1006");
    assert_eq!(lowered.desks[0].responder_mode, ResponderMode::Auto);
    assert_eq!(lowered.coordinator.round_width, 5);
    assert_eq!(lowered.seats.len(), 5);
    for seat in &lowered.seats {
        assert_eq!(seat.template, "pe1006");
        assert_eq!(
            seat.tools.allow,
            [
                "file_read",
                "file_write",
                "mcp_list_tools",
                "mcp_call_tool",
                "shell"
            ]
        );
        assert_eq!(seat.tools.deny, ["run_code", "ask_docs"]);
        assert_eq!(seat.mcp, ["tinyhive"]);
        assert_eq!(seat.config["max_iterations"], 12);
        assert_eq!(seat.config["temperature"], 0.0);
        assert_eq!(seat.model, None);
        if seat.id == "researcher" {
            assert!(
                seat.prompt
                    .contains("You are the only seat allowed to access the public web.")
            );
        } else {
            assert!(seat.prompt.contains("Do not search the web"));
        }
    }
    Ok(())
}
