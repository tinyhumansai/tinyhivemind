//! `live_*`: ACP seats, alone and beside an `OpenHuman` seat, in a real hive.
//!
//! Each seat runs a pinned agent through its runner, is registered with a
//! real coordinator and reaches its hive tools through a real
//! `tinyhivemind-mcp` server, with its own bearer. Only the model is
//! scripted: `hive/model.rs` speaks the three model APIs, reads each
//! rendered turn and calls `hivemind_post`, the same call again, then
//! `hivemind_complete`.
//!
//! Opt in with `THM_NODE` (a Node 22+ binary) and, per agent,
//! `THM_ACP_CLAUDE` (the `claude-agent-acp` script), `THM_ACP_CODEX` (the
//! `codex-acp` script) with `THM_CODEX` (its Codex binary, for the model
//! catalog), and `THM_OPENHUMAN_CORE` for the mixed hive. A test whose
//! agents are missing says so on stderr and passes.
#![cfg(target_os = "linux")]

#[path = "hive/model.rs"]
mod model;

use anyhow::{Context as _, Result, ensure};
use model::{HiveModel, Seen};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tinyhivemind_acp::agent::write_codex_catalog;
use tinyhivemind_acp::{AcpHarness, Agent};
use tinyhivemind_hives::{
    AgentRegistration, Coordinator, CoordinatorOptions, Destination, EpisodePhase, HiveInfo,
    MemoryStorage, Message, SendMessage,
};
use tinyhivemind_host::harness::{Harness, SeatPlan};
use tinyhivemind_lang::{
    LoweredModel, LoweredSeat, LoweredSeatIntegration, Model, Provider, ProviderKind,
};
use tinyhivemind_mcp::{HiveToolServer, SeatTools, ServerOptions, Services};

/// The pinned versions, as each agent reports itself.
const CLAUDE_ACP: &str = "0.112.0";
const CODEX_ACP: &str = "2.12.1";
const OPENHUMAN: &str = "0.64.15";
/// Long enough for every seat to start and finish its turns.
const SETTLE: Duration = Duration::from_secs(300);

fn var(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

/// One seat of the hive: its harness and its lowered seat.
struct Member {
    id: &'static str,
    harness: Arc<dyn Harness>,
    seat: LoweredSeat,
}

/// Seat `id` on a `kind` provider at `endpoint`, with no credential.
fn seat(id: &str, kind: ProviderKind, endpoint: &str) -> LoweredSeat {
    let provider = Provider {
        id: "local".into(),
        kind,
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
        config: serde_json::Value::Null,
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

fn claude(port: u16) -> Option<Member> {
    let agent = Agent::claude_code(var("THM_NODE")?, var("THM_ACP_CLAUDE")?, CLAUDE_ACP);
    let endpoint = format!("http://127.0.0.1:{port}");
    Some(Member {
        id: "claude",
        harness: Arc::new(AcpHarness::new(agent)),
        seat: seat("claude", ProviderKind::Anthropic, &endpoint),
    })
}

fn codex(port: u16, root: &Path) -> Result<Option<Member>> {
    let (Some(node), Some(adapter), Some(binary)) =
        (var("THM_NODE"), var("THM_ACP_CODEX"), var("THM_CODEX"))
    else {
        return Ok(None);
    };
    let catalog = root.join("catalog.json");
    write_codex_catalog(&binary, &catalog, "mock")?;
    let agent = Agent::codex(node, adapter, CODEX_ACP, &catalog);
    let endpoint = format!("http://127.0.0.1:{port}/v1");
    Ok(Some(Member {
        id: "codex",
        harness: Arc::new(AcpHarness::new(agent)),
        seat: seat("codex", ProviderKind::Openai, &endpoint),
    }))
}

fn openhuman(port: u16) -> Option<Member> {
    use tinyhivemind_openhuman_process::{OpenHumanHarness, Options, Program};
    let options = Options {
        ready: Duration::from_secs(60),
        ..Options::default()
    };
    let harness = OpenHumanHarness::new(Program::new(var("THM_OPENHUMAN_CORE")?, OPENHUMAN))
        .with_options(options);
    let endpoint = format!("http://127.0.0.1:{port}/v1");
    Some(Member {
        id: "openhuman",
        harness: Arc::new(harness),
        seat: seat("openhuman", ProviderKind::Openai, &endpoint),
    })
}

/// What seat `id` must have done: posted once under its own name though the
/// post was sent twice, seen its turn and the hive tools on every model call,
/// and been answered the first result for its retry.
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
            "a turn of {id} reached the model without its id"
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

/// Run one episode with every member as a starter, and check each seat and
/// the episode; then stop every seat.
async fn hive(members: Vec<Member>, model: &HiveModel, root: &Path) -> Result<()> {
    let coordinator = Coordinator::new(
        "runtime".into(),
        Arc::new(MemoryStorage::new()),
        CoordinatorOptions::default(),
    )
    .await?;
    let server =
        HiveToolServer::start(Services::new(coordinator.clone()), ServerOptions::default()).await?;
    let ids: Vec<String> = members.iter().map(|member| member.id.to_owned()).collect();
    for member in &members {
        let tools = server.serve_seat(SeatTools::new(member.id))?;
        let plan = SeatPlan::new(member.seat.clone(), root.join(member.id));
        let runner = member
            .harness
            .start(plan, tools)
            .await
            .with_context(|| format!("starting {}", member.id))?;
        coordinator
            .register_agent(AgentRegistration {
                agent_id: member.id.into(),
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
            members: ids.clone(),
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
            starters: ids.clone(),
        })
        .await?;
    tokio::time::timeout(SETTLE, coordinator.run_until_idle())
        .await
        .context("the hive did not settle in time")??;
    let transcript = coordinator.read_transcript(None)?;
    let seen = model.seen();
    for id in &ids {
        seat_took_its_turn(id, &transcript, &seen)?;
    }
    let episodes = coordinator.episodes()?;
    ensure!(
        episodes.len() == 1 && episodes[0].phase == EpisodePhase::Settled,
        "the episode did not settle: {episodes:#?}"
    );
    for member in &members {
        member.harness.stop(member.id).await?;
    }
    server.shutdown().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn live_a_claude_code_seat_posts_once_and_settles() -> Result<()> {
    let model = HiveModel::start().await?;
    let Some(member) = claude(model.port) else {
        eprintln!("skipped: set THM_NODE and THM_ACP_CLAUDE");
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    hive(vec![member], &model, root.path()).await
}

#[tokio::test(flavor = "multi_thread")]
async fn live_a_codex_seat_posts_once_and_settles() -> Result<()> {
    let model = HiveModel::start().await?;
    let root = tempfile::tempdir()?;
    let Some(member) = codex(model.port, root.path())? else {
        eprintln!("skipped: set THM_NODE, THM_ACP_CODEX and THM_CODEX");
        return Ok(());
    };
    hive(vec![member], &model, root.path()).await
}

/// One episode, three harnesses: `OpenHuman` over its own API, Claude Code
/// and Codex over ACP, each seat posting once as itself.
#[tokio::test(flavor = "multi_thread")]
async fn live_a_mixed_hive_of_openhuman_claude_code_and_codex_settles() -> Result<()> {
    let model = HiveModel::start().await?;
    let root = tempfile::tempdir()?;
    let (Some(openhuman), Some(claude), Some(codex)) = (
        openhuman(model.port),
        claude(model.port),
        codex(model.port, root.path())?,
    ) else {
        eprintln!(
            "skipped: set THM_OPENHUMAN_CORE, THM_NODE, THM_ACP_CLAUDE, THM_ACP_CODEX and THM_CODEX"
        );
        return Ok(());
    };
    hive(vec![openhuman, claude, codex], &model, root.path()).await?;
    let apis: std::collections::BTreeSet<String> = model
        .seen()
        .iter()
        .map(|seen| format!("{:?}", seen.api))
        .collect();
    ensure!(
        apis.len() == 3,
        "the hive spoke {apis:?}, not all three model APIs"
    );
    Ok(())
}
