//! A hive package deploys with its executable tools and script hooks.
use super::{
    support::*,
    turns::{Script, mount},
};
use crate::{deploy::*, language::PackageSource};
use serde_json::{Value, json};
use std::sync::{Arc, PoisonError};
use tinyhivemind_hives::{Destination, SendMessage};

/// The `integrations` example, with MCP off (its server is not reachable
/// offline), its provider and models left to the offline fixture, and its hook
/// matching the Python tool the script calls.
fn package() -> anyhow::Result<PackageSource> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tinyhivemind-lang/examples/hives/integrations");
    let mut package = PackageSource::read(&root)?;
    package.manifest.features.insert("mcp".into(), false);
    package.manifest.runtime.provider = None;
    // Plain model names: the scripted model reads plain-text turns, and a
    // provider-prefixed Anthropic name switches the harness to cache-control
    // content blocks.
    // A dollar budget needs a priced model; the fixture has no price.
    package.manifest.runtime.limits.budget_usd = None;
    for model in &mut package.manifest.models {
        model.provider = None;
        model.name = "fixture".into();
    }
    package.manifest.hooks[0].matcher = Some("word_count".into());
    Ok(PackageSource::new(root, &package)?)
}

/// Every tool result the scripted model received, by the tool it answered.
async fn results(model: &wiremock::MockServer) -> Vec<(String, String)> {
    let mut names = std::collections::BTreeMap::<String, String>::new();
    let mut found = Vec::new();
    for request in model.received_requests().await.unwrap_or_default() {
        let body: Value = serde_json::from_slice(&request.body).unwrap_or_default();
        for message in body["messages"].as_array().into_iter().flatten() {
            for call in message["tool_calls"].as_array().into_iter().flatten() {
                names.insert(
                    call["id"].as_str().unwrap_or_default().to_owned(),
                    call["function"]["name"]
                        .as_str()
                        .unwrap_or_default()
                        .to_owned(),
                );
            }
            if message["role"] == "tool"
                && let Some(name) = names.get(message["tool_call_id"].as_str().unwrap_or_default())
            {
                let text = message["content"].as_str().unwrap_or_default().to_owned();
                if !found.contains(&(name.to_owned(), text.clone())) {
                    found.push((name.to_owned(), text));
                }
            }
        }
    }
    found
}

#[test]
fn a_package_seat_runs_its_python_tool_and_a_script_hook_refuses_a_call() -> anyhow::Result<()> {
    let _lock = crate::RUNTIME_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    run(async {
        let (options, _backend, model) = build_options().await;
        let script = Script {
            action: Some("word_count".into()),
            arguments: Some(json!({"text": "please git push --force now"})),
            ..Default::default()
        };
        mount(&model, script).await;
        let deployment = Box::pin(HiveDeployment::from_packages(
            vec![package()?],
            Arc::new(Secrets),
            options.clone(),
        ))
        .await?;
        assert_eq!(
            deployment.seats().keys().collect::<Vec<_>>(),
            ["lead", "scout"]
        );
        let send = |id: &str, seat: &str| SendMessage {
            message_id: id.into(),
            sender: String::new(),
            destination: Destination::Hive("integrations".into()),
            body: "Count the words".into(),
            thread: None,
            only_for: Vec::new(),
            starters: vec![seat.into()],
        };
        // The scout has no hooks: its Python tool runs and counts five words.
        deployment
            .host()
            .coordinator()
            .send_as_host(send("scout", "scout"))
            .await?;
        let report = deployment.host().coordinator().run_until_idle().await?;
        assert_eq!(report.failed, 0, "{report:?}");
        let seen = results(&model).await;
        assert!(
            seen.iter().any(|(name, text)| name == "word_count"
                && (text.contains("\"words\": 5") || text.contains("\"words\":5"))),
            "{seen:?}"
        );
        // The lead's pre_tool_use hook refuses the same call, and a refused
        // call ends the turn, as any refusal at the hive gate does.
        deployment
            .host()
            .coordinator()
            .send_as_host(send("lead", "lead"))
            .await?;
        let report = deployment.host().coordinator().run_until_idle().await?;
        assert_eq!(report.failed, 1, "{report:?}");
        let counted = results(&model)
            .await
            .into_iter()
            .filter(|(name, _)| name == "word_count")
            .count();
        assert_eq!(counted, 1, "the refused call never ran");
        drop(deployment);
        // Control: with the hooks feature off the same call by the lead runs.
        let mut unhooked = package()?.package().clone();
        unhooked.manifest.features.insert("hooks".into(), false);
        let root = package()?.root().to_path_buf();
        let deployment = Box::pin(HiveDeployment::from_packages(
            vec![PackageSource::new(root, &unhooked)?],
            Arc::new(Secrets),
            options,
        ))
        .await?;
        deployment
            .host()
            .coordinator()
            .send_as_host(send("lead-unhooked", "lead"))
            .await?;
        let report = deployment.host().coordinator().run_until_idle().await?;
        assert_eq!(report.failed, 0, "{report:?}");
        Ok(())
    })
}

#[test]
fn a_package_seat_runs_a_shell_tool_and_an_unknown_group_is_refused() -> anyhow::Result<()> {
    let _lock = crate::RUNTIME_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    run(async {
        let (options, _backend, model) = build_options().await;
        let script = Script {
            action: Some("grep_repo".into()),
            arguments: Some(json!({"pattern": "release-notes", "path": "."})),
            ..Default::default()
        };
        mount(&model, script).await;
        let mut source = package()?.package().clone();
        source.manifest.tools[0].cwd = tinyhivemind_lang::ToolDirectory::Package;
        let root = package()?.root().to_path_buf();
        let deployment = Box::pin(HiveDeployment::from_packages(
            vec![PackageSource::new(&root, &source)?],
            Arc::new(Secrets),
            options.clone(),
        ))
        .await?;
        deployment
            .host()
            .coordinator()
            .send_as_host(SendMessage {
                message_id: "grep".into(),
                sender: String::new(),
                destination: Destination::Hive("integrations".into()),
                body: "Find the skill".into(),
                thread: None,
                only_for: Vec::new(),
                starters: vec!["scout".into()],
            })
            .await?;
        let report = deployment.host().coordinator().run_until_idle().await?;
        assert_eq!(report.failed, 0, "{report:?}");
        let seen = results(&model).await;
        assert!(
            seen.iter()
                .any(|(name, text)| name == "grep_repo" && text.contains("hive.json")),
            "{seen:?}"
        );
        drop(deployment);
        source.manifest.toolpacks[0]
            .groups
            .insert("no-such-group".into(), tinyhivemind_lang::GroupMode::Off);
        let refused = Box::pin(HiveDeployment::from_packages(
            vec![PackageSource::new(&root, &source)?],
            Arc::new(Secrets),
            options,
        ))
        .await;
        assert!(
            refused.is_err_and(|error| error.to_string().contains("no-such-group")),
            "an unknown harness tool group is refused"
        );
        Ok(())
    })
}

/// A package from `examples/hives/<name>` of the language crate.
fn example(name: &str) -> anyhow::Result<PackageSource> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tinyhivemind-lang/examples/hives")
        .join(name);
    let package = PackageSource::read(&root)?;
    Ok(PackageSource::new(root, &package)?)
}

/// `mixed-harnesses` with its one remaining harness, `claw`, made an
/// `openhuman` harness every seat but the lead names.
fn openhuman_only() -> anyhow::Result<PackageSource> {
    let source = example("mixed-harnesses")?;
    let mut package = source.package().clone();
    package.manifest.harnesses.truncate(1);
    package.manifest.harnesses[0].kind = tinyhivemind_lang::HarnessKind::OpenHuman;
    package.manifest.harnesses[0].isolation = None;
    package.manifest.seats[2].harness = Some("claw".into());
    package.manifest.profiles[0].harness = None;
    Ok(PackageSource::new(source.root(), &package)?)
}

#[test]
fn a_seat_on_any_harness_but_openhuman_is_refused_before_deployment() -> anyhow::Result<()> {
    let refused = crate::deploy::packages::harnesses(&[example("mixed-harnesses")?]);
    assert!(
        matches!(
            &refused,
            Err(DeployError::UnsupportedHarness { seat, kind })
                if seat == "scout" && *kind == tinyhivemind_lang::HarnessKind::OpenClaw
        ),
        "{refused:?}"
    );
    assert_eq!(
        refused.err().map(|error| error.to_string()).as_deref(),
        Some("seat scout runs on the openclaw harness, which the linked adapter cannot start")
    );
    // No harness anywhere, or an explicit `openhuman` one, deploys as today.
    crate::deploy::packages::harnesses(&[example("integrations")?])?;
    crate::deploy::packages::harnesses(&[openhuman_only()?])?;
    Ok(())
}

#[test]
fn packages_must_agree_on_a_harness_declared_under_one_id() -> anyhow::Result<()> {
    let one = openhuman_only()?;
    let mut package = one.package().clone();
    package.manifest.harnesses[0].kind = tinyhivemind_lang::HarnessKind::Codex;
    let other = PackageSource::new(one.root(), &package)?;
    crate::deploy::packages::harnesses(&[one.clone(), one.clone()])?;
    let refused = crate::deploy::packages::harnesses(&[one, other]);
    assert!(
        matches!(
            &refused,
            Err(DeployError::Config(crate::config::ConfigError::ConflictingDeclaration { section, id }))
                if section == "harnesses" && id == "claw"
        ),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn packages_that_mean_the_same_isolation_agree() -> anyhow::Result<()> {
    let one = openhuman_only()?;
    let mut package = one.package().clone();
    // `seat` is what an `openhuman` harness takes when it says nothing.
    package.manifest.harnesses[0].isolation = Some(tinyhivemind_lang::Isolation::Seat);
    let explicit = PackageSource::new(one.root(), &package)?;
    crate::deploy::packages::harnesses(&[one, explicit])?;
    Ok(())
}

#[test]
fn from_packages_refuses_a_seat_on_another_harness_before_starting_anything() -> anyhow::Result<()> {
    let _lock = crate::RUNTIME_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    run(async {
        let (options, _backend, _model) = build_options().await;
        let refused = Box::pin(HiveDeployment::from_packages(
            vec![example("mixed-harnesses")?],
            Arc::new(Secrets),
            options,
        ))
        .await;
        assert!(
            matches!(refused, Err(DeployError::UnsupportedHarness { ref seat, .. }) if seat == "scout"),
            "{refused:?}"
        );
        Ok(())
    })
}

#[test]
fn a_judged_seat_turns_every_tool_group_its_toolpacks_do_not_list_off() -> anyhow::Result<()> {
    use openhuman_embed::{GroupMode, ToolGroups};
    let runtime = ToolGroups::advertised();
    let listed = std::collections::BTreeMap::from([(
        ToolGroups::ids().next().unwrap_or_default().to_owned(),
        tinyhivemind_lang::GroupMode::Withheld,
    )]);
    let judged = crate::deploy::profile::seat_groups(
        &runtime,
        &listed,
        Some(tinyhivemind_lang::GroupMode::Off),
    )?
    .ok_or_else(|| anyhow::anyhow!("a judged seat sets its groups"))?;
    for id in ToolGroups::ids() {
        let expected = if listed.contains_key(id) {
            GroupMode::Withheld
        } else {
            GroupMode::Off
        };
        assert_eq!(judged.mode(id), expected, "{id}");
    }
    // A seat of a package that names no harness keeps today's behaviour.
    let unjudged = crate::deploy::profile::seat_groups(&runtime, &Default::default(), None)?;
    assert_eq!(unjudged, None);
    Ok(())
}

#[test]
fn a_judged_openhuman_seat_deploys_with_no_built_in_tool_group() -> anyhow::Result<()> {
    let _lock = crate::RUNTIME_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    run(async {
        let (options, _backend, model) = build_options().await;
        mount(&model, Script::default()).await;
        let source = openhuman_only()?;
        let mut package = source.package().clone();
        // The check judges the provider each seat's model names; the linked
        // adapter uses the runtime's, so leaving that one to the fixture's
        // injected provider deploys the same judged seats offline.
        package.manifest.runtime.provider = None;
        let deployment = Box::pin(HiveDeployment::from_packages(
            vec![PackageSource::new(source.root(), &package)?],
            Arc::new(Secrets),
            options,
        ))
        .await?;
        deployment
            .host()
            .coordinator()
            .send_as_host(SendMessage {
                message_id: "tools".into(),
                sender: String::new(),
                destination: Destination::Hive("mixed-harnesses".into()),
                body: "Say hello".into(),
                thread: None,
                only_for: Vec::new(),
                starters: vec!["lead".into()],
            })
            .await?;
        let report = deployment.host().coordinator().run_until_idle().await?;
        assert_eq!(report.failed, 0, "{report:?}");
        let mut offered = std::collections::BTreeSet::new();
        for request in model.received_requests().await.unwrap_or_default() {
            let body: Value = serde_json::from_slice(&request.body).unwrap_or_default();
            for tool in body["tools"].as_array().into_iter().flatten() {
                if let Some(name) = tool["function"]["name"].as_str() {
                    offered.insert(name.to_owned());
                }
            }
        }
        assert!(offered.contains("word_count"), "{offered:?}");
        let none = openhuman_embed::ToolGroups::none();
        let built_in: Vec<_> = offered
            .iter()
            .filter(|name| {
                *name == "use_skill" || none.mode_for_tool(name) == openhuman_embed::GroupMode::Off
            })
            .collect();
        assert!(built_in.is_empty(), "built-in tools reached the model: {built_in:?}");
        Ok(())
    })
}
