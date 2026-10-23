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
    for model in &mut package.manifest.models {
        model.provider = None;
        model.name = "fixture".into();
    }
    package.manifest.hooks[0].matcher = Some("word_count".into());
    Ok(PackageSource::new(root, package)?)
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
        let _ = env_logger::builder().is_test(true).try_init();
        let (mut options, _backend, model) = build_options().await;
        // The package's dollar budget needs the host's per-call bound.
        options.call_budget = Some(openhuman_embed::budget::CallBudget {
            input_tokens: 100_000,
            output_tokens: 4_096,
            cost_micros: 10_000,
        });
        let script = Script {
            action: Some("word_count".into()),
            arguments: Some(json!({"text": "please git push --force now"})),
            ..Default::default()
        };
        mount(&model, script).await;
        let deployment = Box::pin(HiveDeployment::from_packages(
            vec![package()?],
            Arc::new(Secrets),
            options,
        ))
        .await?;
        assert_eq!(
            deployment.seats().keys().collect::<Vec<_>>(),
            ["lead", "scout"]
        );
        deployment
            .host()
            .coordinator()
            .send_as_host(SendMessage {
                message_id: "release".into(),
                sender: String::new(),
                destination: Destination::Hive("integrations".into()),
                body: "Count the words".into(),
                thread: None,
                only_for: Vec::new(),
                starters: vec!["lead".into(), "scout".into()],
            })
            .await?;
        let report = deployment.host().coordinator().run_until_idle().await?;
        eprintln!("DBG requests={} seen={:?}", model.received_requests().await.unwrap_or_default().len(), results(&model).await);
        for row in deployment.host().coordinator().read_transcript(None)? { eprintln!("DBG ROW {row:?}"); }
        assert_eq!(report.failed, 0, "{report:?}");
        let seen = results(&model).await;
        let counts: Vec<_> = seen
            .iter()
            .filter(|(name, _)| name == "word_count")
            .collect();
        // The scout has no hooks: its Python tool ran and counted five words.
        assert!(
            counts
                .iter()
                .any(|(_, text)| text.contains("\"words\": 5") || text.contains("\"words\":5")),
            "{seen:?}"
        );
        // The lead's pre_tool_use hook refused the same call.
        assert!(
            counts
                .iter()
                .any(|(_, text)| text.contains("destructive git commands")),
            "{seen:?}"
        );
        Ok(())
    })
}

#[test]
fn a_package_seat_runs_a_shell_tool_and_an_unknown_group_is_refused() -> anyhow::Result<()> {
    let _lock = crate::RUNTIME_LOCK
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    run(async {
        let (mut options, _backend, model) = build_options().await;
        // The package's dollar budget needs the host's per-call bound.
        options.call_budget = Some(openhuman_embed::budget::CallBudget {
            input_tokens: 100_000,
            output_tokens: 4_096,
            cost_micros: 10_000,
        });
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
            vec![PackageSource::new(&root, source.clone())?],
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
            vec![PackageSource::new(&root, source)?],
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
