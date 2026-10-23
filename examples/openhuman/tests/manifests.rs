//! Checked-in packages preserve the seats, policy and native tool boundaries.
use tinyhivemind_lang::{Access, LoweredHive, MemoryEngine, Package};
use tinyhivemind_openhuman::language::PackageSource;

/// Every checked-in hive, as the package directories one deployment takes.
const DEPLOYMENTS: [&[&str]; 4] = [
    &["basic_hive"],
    &["deepswe_hive"],
    &["pe1006_hive"],
    &["multi_hive/hive-a", "multi_hive/hive-b"],
];

fn load(name: &str) -> (Package, LoweredHive) {
    let source = PackageSource::load(tinyhivemind_openhuman_example::root(name)).unwrap();
    (source.package().clone(), source.lowered().clone())
}

fn seat_ids(hive: &LoweredHive) -> Vec<&str> {
    hive.seats.iter().map(|s| s.id.as_str()).collect()
}

#[test]
fn every_package_checks_and_its_canonical_files_are_complete() {
    for name in DEPLOYMENTS.iter().flat_map(|names| names.iter()) {
        let root = tinyhivemind_openhuman_example::root(name);
        let package = tinyhivemind_openhuman_example::manifest(name).unwrap();
        assert!(tinyhivemind_lang::diagnose(&package).is_empty(), "{name}");
        for (file, canonical) in [
            ("hive.json", serde_json::to_string_pretty(&package.manifest)),
            ("constitution.json", serde_json::to_string_pretty(&package.constitution)),
            ("memory.json", serde_json::to_string_pretty(&package.memory)),
        ] {
            let written = std::fs::read_to_string(root.join(file)).unwrap();
            let canonical = canonical.unwrap() + "\n";
            assert_eq!(written, canonical, "{name}/{file} is not canonical");
        }
    }
}

#[test]
fn basic_preserves_two_readonly_seats_and_ephemeral_workspace() {
    let (package, hive) = load("basic_hive");
    assert_eq!(seat_ids(&hive), ["alice", "bob"]);
    assert_eq!(package.manifest.runtime.autonomy, Access::ReadOnly);
    assert!(package.manifest.runtime.workspace.is_none());
    assert_eq!(hive.desks[0].members.len(), 2);
    assert!(
        hive.seats
            .iter()
            .all(|seat| seat.prompt.contains("hivemind_complete"))
    );
}

#[test]
fn deepswe_preserves_roles_mcp_iterations_sampling_and_width() {
    let (package, hive) = load("deepswe_hive");
    assert_eq!(seat_ids(&hive), ["lead", "implementer", "tester", "reviewer"]);
    assert_eq!(hive.policies.episode.round_width, 4);
    assert_eq!(hive.coordinator.round_width, 4);
    assert_eq!(package.manifest.runtime.concurrency, 4);
    for seat in &hive.seats {
        let integration = &seat.integration;
        assert_eq!(integration.limits.iterations, Some(16));
        let model = integration.model.as_ref().unwrap();
        assert_eq!(model.model.temperature, Some(0.0));
        assert_eq!(integration.tools.allow, ["mcp_list_tools", "mcp_call_tool"]);
        let servers: Vec<_> = integration.mcp.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(servers, ["deepswe", &format!("tinyhive-{}", seat.id)]);
        assert_eq!(
            integration.mcp[0].tools.as_ref().unwrap(),
            &["file_read", "file_write", "file_edit", "shell", "test"]
        );
        assert_eq!(
            integration.mcp[1].tools.as_ref().unwrap(),
            &["broadcast", "complete_episode"]
        );
        assert!(seat.prompt.contains(&format!("server tinyhive-{},", seat.id)));
    }
}

#[test]
fn euler_preserves_five_roles_native_rules_limits_and_workspace_templates() {
    let (package, hive) = load("pe1006_hive");
    assert_eq!(hive.seats.len(), 5);
    assert_eq!(package.manifest.runtime.memory_engine, MemoryEngine::Host);
    assert_eq!(hive.policies.routing.round_width, 5);
    assert_eq!(hive.coordinator.round_width, 4);
    for seat in &hive.seats {
        let integration = &seat.integration;
        assert_eq!(integration.limits.iterations, Some(12));
        let model = integration.model.as_ref().unwrap();
        assert_eq!(model.model.temperature, Some(0.0));
        assert_eq!(integration.tools.deny, ["run_code", "ask_docs"]);
        assert_eq!(
            integration.tools.allow,
            [
                "file_read",
                "file_write",
                "mcp_list_tools",
                "mcp_call_tool",
                "shell"
            ]
        );
        assert_eq!(integration.mcp.len(), 1);
        assert_eq!(integration.mcp[0].id, format!("tinyhive-{}", seat.id));
        assert!(
            integration.mcp[0]
                .tools
                .as_ref()
                .unwrap()
                .contains(&"hive_memory_note".into())
        );
        let context = if seat.id == "researcher" {
            "context/research.md"
        } else {
            "context/sealed.md"
        };
        assert_eq!(integration.context[0].0, context);
    }
    let document = |path: &str| &package.documents[path];
    assert!(document("context/agents.md").contains("Never overwrite another role"));
    assert!(document("context/memory.md").contains("Psi(3) = 20302"));
    assert!(document("context/task.md").contains("Psi(10^18)"));
}

#[test]
fn multi_shares_one_seat_with_distinct_roles_and_narrower_second_hive() {
    let (a_package, a) = load("multi_hive/hive-a");
    let (b_package, b) = load("multi_hive/hive-b");
    assert_eq!(seat_ids(&a), ["shared", "architect", "helper"]);
    assert_eq!(seat_ids(&b), ["shared", "reviewer"]);
    let declared = |package: &Package| {
        package
            .manifest
            .seats
            .iter()
            .find(|seat| seat.id == "shared")
            .unwrap()
            .clone()
    };
    let (in_a, in_b) = (declared(&a_package), declared(&b_package));
    assert_ne!(in_a.role, in_b.role);
    assert!(in_a.permissions.is_none());
    assert_eq!(in_b.permissions.as_deref(), Some("readonly"));
    assert_eq!(a.seats[0].prompt, b.seats[0].prompt);
    assert_eq!(a_package.manifest.runtime, b_package.manifest.runtime);
}

#[test]
fn all_packages_lower_into_one_native_runtime_with_the_declared_seats_and_limits()
-> anyhow::Result<()> {
    use tinyhivemind_openhuman::deploy::{BuildOptions, native_memory_root};
    let executor = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(16 * 1024 * 1024)
        .build()?;
    executor.block_on(async {
        tokio::spawn(async {
            let backend = tinyhivemind_openhuman::offline::backend().await;
            for names in DEPLOYMENTS {
                let mut packages = Vec::new();
                for name in names {
                    let mut package = tinyhivemind_openhuman_example::manifest(name)?;
                    // A deterministic reference engine substitutes for operator
                    // CortexDB credentials. Provider and MCP connections are
                    // never invoked here.
                    if *name == "pe1006_hive" {
                        package.manifest.runtime.memory_engine = MemoryEngine::Memory;
                    }
                    package.manifest.runtime.provider = None;
                    packages.push((*name, package));
                }
                let expected: Vec<LoweredHive> = packages
                    .iter()
                    .map(|(_, package)| tinyhivemind_lang::lower(package))
                    .collect::<Result<_, _>>()?;
                let deployment_id = packages[0]
                    .1
                    .manifest
                    .runtime
                    .deployment_id
                    .clone()
                    .unwrap_or_else(|| expected[0].id.clone());
                let deployment = tinyhivemind_openhuman_example::deploy(
                    packages,
                    BuildOptions {
                        config: Some(tinyhivemind_openhuman::offline::config()),
                        backend_url: Some(backend.uri()),
                        provider: Some(
                            openhuman_embed::Provider::openai_compatible(
                                "http://127.0.0.1:1/v1",
                                "fixture",
                            )
                            .model("fixture"),
                        ),
                        ..Default::default()
                    },
                )
                .await?;
                let mut seats: Vec<_> = expected.iter().flat_map(|hive| &hive.seats).collect();
                seats.sort_by(|x, y| x.id.cmp(&y.id));
                seats.dedup_by(|x, y| x.id == y.id);
                let label = names.join("+");
                assert_eq!(deployment.seats().len(), seats.len(), "{label}");
                assert_eq!(
                    deployment.host().coordinator().list_hives()?.len(),
                    expected.len(),
                    "{label}"
                );
                let memory_root = native_memory_root(&format!("deployment:{deployment_id}"))?;
                for seat in seats {
                    let native = &deployment.seats()[&seat.id];
                    let integration = &seat.integration;
                    assert_eq!(native.runtime_id(), deployment.runtime().runtime_id());
                    assert_eq!(native.id(), format!("{deployment_id}-{}", seat.id));
                    assert_eq!(
                        native.config().memory.root.as_deref(),
                        Some(memory_root.as_str())
                    );
                    if let Some(iterations) = integration.limits.iterations {
                        assert_eq!(
                            native.config().agent.max_tool_iterations_override,
                            Some(iterations),
                            "{label}:{}",
                            seat.id
                        );
                    }
                    if let Some(temperature) = integration
                        .model
                        .as_ref()
                        .and_then(|model| model.model.temperature)
                    {
                        assert_eq!(
                            native.config().default_temperature,
                            temperature,
                            "{label}:{}",
                            seat.id
                        );
                    }
                }
                for (hive, package) in expected.iter().zip(names.iter()) {
                    let declared = tinyhivemind_openhuman_example::manifest(package)?;
                    let settings = deployment
                        .host()
                        .coordinator()
                        .hive_settings(&hive.id)?
                        .unwrap();
                    for seat in &declared.manifest.seats {
                        assert_eq!(settings.roles[&seat.id], seat.role);
                    }
                    assert_eq!(
                        deployment.policy(&hive.id).unwrap().episode,
                        hive.policies.episode
                    );
                }
                drop(deployment);
            }
            Ok::<_, anyhow::Error>(())
        })
        .await?
    })
}
