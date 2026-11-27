//! `live_hive_*`: seats of this runner, on the pinned `openhuman-core`, in a
//! real hive.
//!
//! Each seat runs the pinned `openhuman-core` through this crate's runner, is
//! registered with a real [`Coordinator`], and reaches its hive tools through
//! a real [`HiveToolServer`], with its own bearer. Nothing here is a stub but
//! the model: `hive/model.rs` reads each rendered turn and calls
//! `hivemind_post`, retries that same call, then calls `hivemind_complete`.
//!
//! Opt in with `THM_OPENHUMAN_CORE=<path to the pinned binary>`, fetched and
//! checked by `.github/scripts/fetch-harness.sh`. Without it each test says
//! why on stderr and passes.
#![cfg(target_os = "linux")]

#[path = "hive/model.rs"]
mod model;

use anyhow::{Context as _, Result, ensure};
use model::{HiveModel, Seen};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tinyhivemind_hives::{
    AgentRegistration, Coordinator, CoordinatorOptions, Destination, EpisodePhase, HiveInfo,
    MemoryStorage, Message, SendMessage,
};
use tinyhivemind_host::harness::{Harness as _, SeatPlan};
use tinyhivemind_host::process::StopPolicy;
use tinyhivemind_lang::{
    LoweredModel, LoweredSeat, LoweredSeatIntegration, Model, Provider, ProviderKind,
};
use tinyhivemind_mcp::{HiveToolServer, SeatTools, ServerOptions, Services};
use tinyhivemind_openhuman_process::{OpenHumanHarness, Options, Program};

/// The pinned version, as `harnesses.lock` records it.
const PINNED: &str = "0.64.15";
/// The hive's seats, both starters of its one episode.
const SEATS: [&str; 2] = ["lead", "builder"];
/// Long enough for both seats to start, warm up and finish their turns.
const SETTLE: Duration = Duration::from_secs(240);

fn options() -> Options {
    Options {
        // The first start after an idle spell took about 30 s on a loaded
        // shared machine (load average near 100); later starts take 2 s.
        ready: Duration::from_secs(60),
        warm_up: Duration::from_secs(20),
        stop: StopPolicy::default(),
        approval: Duration::from_secs(5),
    }
}

/// Seat `id` on an `openai` endpoint at `endpoint`, with no credential.
fn seat(id: &str, endpoint: &str) -> LoweredSeat {
    let provider = Provider {
        id: "local".into(),
        kind: ProviderKind::Openai,
        endpoint: Some(endpoint.into()),
        credential: None,
    };
    let model = Model {
        id: "fast".into(),
        provider: Some("local".into()),
        name: "mock".into(),
        ..Model::default()
    };
    LoweredSeat {
        id: id.into(),
        label: id.into(),
        template: "seat".into(),
        config: Value::Null,
        prompt: format!("You are {id}. Report, then complete."),
        memory: None,
        integration: LoweredSeatIntegration {
            model: Some(LoweredModel {
                model,
                provider: Some(provider),
            }),
            ..LoweredSeatIntegration::default()
        },
    }
}

/// Whatever runs in `instance`.
fn processes_in(instance: &Path) -> Vec<u32> {
    fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
        .filter(|pid: &u32| {
            fs::read_link(format!("/proc/{pid}/cwd")).is_ok_and(|cwd| cwd.starts_with(instance))
        })
        .collect()
}

/// What seat `id` must have done: posted once under its own name though the
/// post was sent twice, seen its turn id and the hive tools on every model
/// call, and been answered the first result for its retry.
fn seat_took_its_turn(id: &str, transcript: &[Message], seen: &[Seen]) -> Result<()> {
    let posts = transcript
        .iter()
        .filter(|message| message.sender == id && message.body == format!("{id} reporting"))
        .count();
    ensure!(posts == 1, "{id} posted {posts} times: {transcript:#?}");
    let turns: Vec<_> = seen.iter().filter(|request| request.agent == id).collect();
    ensure!(!turns.is_empty(), "the model never saw a turn of {id}");
    for request in &turns {
        ensure!(
            !request.turn.is_empty(),
            "a turn of {id} reached the model without its turn id"
        );
        let offered = |name: &str| request.offered.iter().any(|tool| tool.contains(name));
        ensure!(
            offered("hivemind_post") && offered("hivemind_complete"),
            "{id} was not offered the hive tools: {:?}",
            request.offered
        );
    }
    let retried = turns
        .iter()
        .find(|request| request.results.len() == 2)
        .with_context(|| format!("{id} never saw its retried post answered: {seen:#?}"))?;
    ensure!(
        retried.results[0] == retried.results[1],
        "{id}'s retry was not answered with the first result: {:?}",
        retried.results
    );
    Ok(())
}

/// Two `OpenHuman` processes take one episode together: each posts once
/// although its post is sent twice, each completes, and the episode settles
/// with every message attributed to the seat whose bearer sent it.
#[tokio::test(flavor = "multi_thread")]
async fn live_hive_two_openhuman_seats_post_once_each_and_settle() -> Result<()> {
    let Some(binary) = std::env::var_os("THM_OPENHUMAN_CORE").map(PathBuf::from) else {
        eprintln!("skipped: THM_OPENHUMAN_CORE is not set to the pinned openhuman-core");
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    let model = HiveModel::start().await?;
    let endpoint = format!("http://127.0.0.1:{}/v1", model.port);
    let coordinator = Coordinator::new(
        "runtime".into(),
        Arc::new(MemoryStorage::new()),
        CoordinatorOptions::default(),
    )
    .await?;
    let server =
        HiveToolServer::start(Services::new(coordinator.clone()), ServerOptions::default()).await?;
    let harness = OpenHumanHarness::new(Program::new(binary, PINNED)).with_options(options());
    for id in SEATS {
        let tools = server.serve_seat(SeatTools::new(id))?;
        let plan = SeatPlan::new(seat(id, &endpoint), root.path().join(id));
        let runner = harness.start(plan, tools).await?;
        coordinator
            .register_agent(AgentRegistration {
                agent_id: id.into(),
                runtime_id: coordinator.runtime_id().into(),
                runner,
            })
            .await?;
    }
    coordinator
        .create_hive(HiveInfo {
            hive_id: "work".into(),
            name: "Work".into(),
            description: None,
            members: SEATS.map(Into::into).to_vec(),
        })
        .await?;
    coordinator
        .send_as_host(SendMessage {
            message_id: "task".into(),
            sender: String::new(),
            destination: Destination::Hive("work".into()),
            body: "Report, then complete.".into(),
            thread: None,
            only_for: Vec::new(),
            starters: SEATS.map(Into::into).to_vec(),
        })
        .await?;
    tokio::time::timeout(SETTLE, coordinator.run_until_idle())
        .await
        .context("the hive did not settle in time")??;

    let transcript = coordinator.read_transcript(None)?;
    let seen = model.seen();
    for id in SEATS {
        seat_took_its_turn(id, &transcript, &seen)?;
    }
    let episodes = coordinator.episodes()?;
    ensure!(
        episodes.len() == 1 && episodes[0].phase == EpisodePhase::Settled,
        "the episode did not settle: {episodes:#?}"
    );

    for id in SEATS {
        harness.stop(id).await?;
    }
    server.shutdown().await?;
    for id in SEATS {
        let left = processes_in(&root.path().join(id));
        ensure!(left.is_empty(), "{id} left processes behind: {left:?}");
    }
    Ok(())
}
