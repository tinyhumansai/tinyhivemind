//! `live_*`: hives whose seats run on every harness, started by
//! [`ProcessSeats`]: alone, beside each other, editing through the host
//! (`hive/edit.rs`), in lead-only episodes (`hive/delegation.rs`), and
//! started again on the same coordinator after a start that failed.
//!
//! Each seat names its harness kind; `ProcessSeats` starts it on the pinned
//! agent that kind maps to, registers it with a real coordinator, and serves
//! its hive tools from a real `tinyhivemind-mcp` server with its own bearer,
//! starting the `OpenClaw` Gateway its `OpenClaw` seats share. Only the model
//! is scripted: `hive/model.rs` speaks the three model APIs, reads each
//! rendered turn and calls `hivemind_post`, the same call again (once, then
//! the host's `write_file`, for a seat that edits), then `hivemind_complete`;
//! or, for a lead-only episode, has the lead ask the other harnesses' seats
//! and complete once they have answered.
//!
//! Opt in with `THM_NODE` (a Node 22+ binary) and, per agent,
//! `THM_ACP_CLAUDE` (the `claude-agent-acp` script), `THM_ACP_CODEX` (the
//! `codex-acp` script) with `THM_CODEX` (its Codex binary, for the model
//! catalog), `THM_OPENCLAW` (`OpenClaw`'s `openclaw.mjs`), and
//! `THM_OPENHUMAN_CORE`. A test whose agents are missing says so on stderr and
//! passes.
#![cfg(target_os = "linux")]

#[path = "hive/model.rs"]
mod model;

#[path = "hive/delegation.rs"]
mod delegation;

#[path = "hive/edit.rs"]
mod edit;

use anyhow::{Context as _, Result, ensure};
use model::{EDIT_TOOL, HiveModel, Seen};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tinyhivemind_deploy::{CodexInstall, Deployment, Installs, Pinned, ProcessSeat, ProcessSeats};
use tinyhivemind_exec::ExecContext;
use tinyhivemind_hives::{
    Coordinator, CoordinatorOptions, Destination, EpisodePhase, HiveInfo, MemoryStorage, Message,
    SendMessage,
};
use tinyhivemind_host::harness::Approver;
use tinyhivemind_lang::{
    Access, ExecTool, Harness, HarnessKind, LoweredModel, LoweredSeat, LoweredSeatIntegration,
    Model, Provider, ProviderKind,
};
use tinyhivemind_mcp::{SeatTools, Services};

/// The pinned versions, as each agent reports itself.
const CLAUDE_ACP: &str = "0.112.0";
const CODEX_ACP: &str = "2.12.1";
const OPENHUMAN: &str = "0.64.15";
const OPENCLAW: &str = "2026.10.33";
/// Long enough for every seat to start and finish its turns.
const SETTLE: Duration = Duration::from_secs(300);

fn var(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

/// The pinned agents this machine has, from the environment.
fn installs() -> Installs {
    let codex = var("THM_ACP_CODEX")
        .zip(var("THM_CODEX"))
        .map(|(adapter, binary)| CodexInstall {
            adapter: Pinned::new(adapter, CODEX_ACP),
            binary,
        });
    Installs {
        node: var("THM_NODE"),
        openhuman: var("THM_OPENHUMAN_CORE").map(|path| Pinned::new(path, OPENHUMAN)),
        claude_code: var("THM_ACP_CLAUDE").map(|path| Pinned::new(path, CLAUDE_ACP)),
        codex,
        openclaw: var("THM_OPENCLAW").map(|path| Pinned::new(path, OPENCLAW)),
    }
}

/// One seat of the hive: its lowered seat, which names its harness, and,
/// when it may edit, the access the host's edit tool runs under.
struct Member {
    id: &'static str,
    seat: LoweredSeat,
    edit: Option<Access>,
}

impl Member {
    /// This seat, with the host's edit tool under `access`.
    fn editing(mut self, access: Access) -> Self {
        self.edit = Some(access);
        self
    }
}

/// Writes `content` to `path` in the seat's workspace, refusing a path that
/// is absolute or climbs out of it, and logs each write it makes to
/// `writes.log` in the package root, one `<seat> <path>` line per run. The
/// host runs it, so every harness edits the same way and the host's access
/// gate judges each call.
const WRITE_FILE: &str = r#"case "$1" in /*|..|../*|*/..|*/../*) echo "outside the workspace: $1" >&2; exit 2;; esac
printf '%s' "$2" > "$1" && printf '%s %s\n' "$THM_SEAT" "$1" >> "$THM_PACKAGE_ROOT/writes.log" && echo "wrote $1""#;

/// The host's edit tool.
fn write_file() -> Result<ExecTool> {
    let declaration = serde_json::json!({
        "id": EDIT_TOOL,
        "description": "Write content to a file in your workspace.",
        "command": ["sh", "-c", WRITE_FILE, EDIT_TOOL],
        "args": {
            "type": "object",
            "properties": {"path": {"type": "string"}, "content": {"type": "string"}},
            "required": ["path", "content"],
        },
        "argv": ["{path}", "{content}"],
        "stdin": "none",
        "side_effects": "write",
        "cwd": "workspace",
    });
    Ok(serde_json::from_value(declaration)?)
}

/// Seat `id`'s workspace, where its edit tool writes.
fn workspace(root: &Path, id: &str) -> PathBuf {
    root.join("workspaces").join(id)
}

/// Seat `id` on `harness`, its model a `provider` endpoint, with no
/// credential.
fn seat(id: &str, harness: HarnessKind, provider: ProviderKind, endpoint: &str) -> LoweredSeat {
    let provider = Provider {
        id: "local".into(),
        kind: provider,
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
            harness: Some(Harness {
                id: harness.to_string(),
                kind: harness,
                isolation: None,
            }),
            ..LoweredSeatIntegration::default()
        },
    }
}

/// Seat `id` on `kind`, if this machine has its agent; its model is the
/// scripted one at `port`, in the API the kind speaks.
fn member(id: &'static str, kind: HarnessKind, port: u16) -> Option<Member> {
    let installs = installs();
    let installed = match kind {
        HarnessKind::OpenHuman => installs.openhuman.is_some(),
        HarnessKind::ClaudeCode => installs.node.is_some() && installs.claude_code.is_some(),
        HarnessKind::Codex => installs.node.is_some() && installs.codex.is_some(),
        HarnessKind::OpenClaw => installs.node.is_some() && installs.openclaw.is_some(),
    };
    let (provider, endpoint) = match kind {
        HarnessKind::ClaudeCode => (ProviderKind::Anthropic, format!("http://127.0.0.1:{port}")),
        _ => (ProviderKind::Openai, format!("http://127.0.0.1:{port}/v1")),
    };
    installed.then(|| Member {
        id,
        seat: seat(id, kind, provider, &endpoint),
        edit: None,
    })
}

fn claude(id: &'static str, port: u16) -> Option<Member> {
    member(id, HarnessKind::ClaudeCode, port)
}

fn codex(id: &'static str, port: u16) -> Option<Member> {
    member(id, HarnessKind::Codex, port)
}

fn openhuman(port: u16) -> Option<Member> {
    member("openhuman", HarnessKind::OpenHuman, port)
}

fn openclaw(id: &'static str, port: u16) -> Option<Member> {
    member(id, HarnessKind::OpenClaw, port)
}

/// What seat `id` must have done: posted once under its own name, seen its
/// turn and the hive tools on every model call, and, unless it edits (whose
/// retry is its write's), been answered the first result for its post's
/// retry.
fn seat_took_its_turn(id: &str, edits: bool, transcript: &[Message], seen: &[Seen]) -> Result<()> {
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
    if edits {
        return Ok(());
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

/// How a hive runs: its coordinator's options, and the host's approver for
/// supervised seats' edits.
#[derive(Default)]
struct Setup {
    options: CoordinatorOptions,
    approver: Option<Arc<dyn Approver>>,
}

/// A started hive.
struct Running {
    coordinator: Coordinator,
    seats: ProcessSeats,
}

/// Run one episode with every member as a starter, and check each seat and
/// the episode; then stop every seat.
async fn hive(members: Vec<Member>, model: &HiveModel, root: &Path) -> Result<()> {
    hive_with(members, model, root, Setup::default()).await
}

/// [`hive`], set up as `setup` says.
async fn hive_with(
    members: Vec<Member>,
    model: &HiveModel,
    root: &Path,
    setup: Setup,
) -> Result<()> {
    let running = start_hive(&members, root, setup).await?;
    let ids = members.iter().map(|member| member.id.to_owned()).collect();
    settle(&running.coordinator, ids).await?;
    let transcript = running.coordinator.read_transcript(None)?;
    let seen = model.seen();
    for member in &members {
        seat_took_its_turn(member.id, member.edit.is_some(), &transcript, &seen)?;
    }
    stop_hive(running).await
}

/// A coordinator set up as `setup` says, with every member started on its
/// harness by [`ProcessSeats`] and in hive `work`. An editing member is
/// served the host's edit tool in its own workspace.
async fn start_hive(members: &[Member], root: &Path, setup: Setup) -> Result<Running> {
    let coordinator = Coordinator::new(
        "runtime".into(),
        Arc::new(MemoryStorage::new()),
        setup.options,
    )
    .await?;
    let mut services = Services::new(coordinator.clone());
    if let Some(approver) = setup.approver {
        services = services.with_approver(approver);
    }
    let mut seats = Vec::new();
    for member in members {
        let mut tools = SeatTools::new(member.id);
        if let Some(access) = member.edit {
            let workspace = workspace(root, member.id);
            std::fs::create_dir_all(&workspace)?;
            let context = ExecContext {
                package_root: root.to_path_buf(),
                workspace: Some(workspace),
                hive: "work".into(),
                seat: member.id.into(),
                ..ExecContext::default()
            };
            tools = tools
                .with_programs(vec![write_file()?])
                .with_context(context)
                .with_access(access);
        }
        let seat = ProcessSeat::new(member.seat.clone(), root.join(member.id));
        seats.push(seat.with_tools(tools));
    }
    let mut deployment = Deployment::new(root.join("deployment"));
    // The first `openhuman-core` start after an idle spell can be slow on a
    // loaded machine.
    deployment.openhuman.ready = Duration::from_secs(60);
    let seats =
        ProcessSeats::start(&coordinator, services, seats, &installs(), &deployment).await?;
    coordinator
        .create_hive(HiveInfo {
            hive_id: "work".into(),
            name: "Work".into(),
            description: None,
            members: members.iter().map(|member| member.id.into()).collect(),
        })
        .await?;
    Ok(Running { coordinator, seats })
}

/// Send the task to hive `work`, opened by `starters`, and wait for the one
/// episode it opens to settle.
async fn settle(coordinator: &Coordinator, starters: Vec<String>) -> Result<()> {
    coordinator
        .send_as_host(SendMessage {
            message_id: "task".into(),
            sender: String::new(),
            destination: Destination::Hive("work".into()),
            body: "Report, then complete.".into(),
            thread: None,
            only_for: Vec::new(),
            starters,
        })
        .await?;
    tokio::time::timeout(SETTLE, coordinator.run_until_idle())
        .await
        .context("the hive did not settle in time")??;
    let episodes = coordinator.episodes()?;
    ensure!(
        episodes.len() == 1 && episodes[0].phase == EpisodePhase::Settled,
        "the episode did not settle: {episodes:#?}"
    );
    Ok(())
}

/// Stop every seat, the Gateways and the tool server.
async fn stop_hive(mut running: Running) -> Result<()> {
    running.seats.stop().await?;
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn live_a_claude_code_seat_posts_once_and_settles() -> Result<()> {
    let model = HiveModel::start().await?;
    let Some(member) = claude("claude", model.port) else {
        eprintln!("skipped: set THM_NODE and THM_ACP_CLAUDE");
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    hive(vec![member], &model, root.path()).await
}

#[tokio::test(flavor = "multi_thread")]
async fn live_a_codex_seat_posts_once_and_settles() -> Result<()> {
    let model = HiveModel::start().await?;
    let Some(member) = codex("codex", model.port) else {
        eprintln!("skipped: set THM_NODE, THM_ACP_CODEX and THM_CODEX");
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    hive(vec![member], &model, root.path()).await
}

/// One episode, three harnesses: `OpenHuman` over its own API, Claude Code
/// and Codex over ACP, each seat posting once as itself.
#[tokio::test(flavor = "multi_thread")]
async fn live_a_mixed_hive_of_openhuman_claude_code_and_codex_settles() -> Result<()> {
    let model = HiveModel::start().await?;
    let (Some(openhuman), Some(claude), Some(codex)) = (
        openhuman(model.port),
        claude("claude", model.port),
        codex("codex", model.port),
    ) else {
        eprintln!(
            "skipped: set THM_OPENHUMAN_CORE, THM_NODE, THM_ACP_CLAUDE, THM_ACP_CODEX and THM_CODEX"
        );
        return Ok(());
    };
    let root = tempfile::tempdir()?;
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

#[tokio::test(flavor = "multi_thread")]
async fn live_an_openclaw_seat_posts_once_and_settles() -> Result<()> {
    let model = HiveModel::start().await?;
    let Some(member) = openclaw("openclaw", model.port) else {
        eprintln!("skipped: set THM_NODE and THM_OPENCLAW");
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    hive(vec![member], &model, root.path()).await
}

/// One episode, four harnesses: `OpenHuman` over its own API, and Claude
/// Code, Codex and `OpenClaw` over ACP, each seat posting once as itself.
#[tokio::test(flavor = "multi_thread")]
async fn live_a_hive_of_all_four_harnesses_settles() -> Result<()> {
    let model = HiveModel::start().await?;
    let (Some(openhuman), Some(claude), Some(codex), Some(openclaw)) = (
        openhuman(model.port),
        claude("claude", model.port),
        codex("codex", model.port),
        openclaw("openclaw", model.port),
    ) else {
        eprintln!(
            "skipped: set THM_OPENHUMAN_CORE, THM_NODE, THM_ACP_CLAUDE, THM_ACP_CODEX, THM_CODEX and THM_OPENCLAW"
        );
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    let members = vec![openhuman, claude, codex, openclaw];
    hive(members, &model, root.path()).await
}

/// A start that fails after a seat registered unregisters it, so the same
/// seat starts again on the same coordinator. The `OpenHuman` seat starts and
/// registers; the Codex seat's adapter is not there, so its start fails.
#[tokio::test(flavor = "multi_thread")]
async fn live_a_failed_start_can_be_retried_on_the_same_coordinator() -> Result<()> {
    let model = HiveModel::start().await?;
    let (Some(first), Some(broken)) = (openhuman(model.port), codex("codex", model.port)) else {
        eprintln!("skipped: set THM_OPENHUMAN_CORE, THM_NODE, THM_ACP_CODEX and THM_CODEX");
        return Ok(());
    };
    let root = tempfile::tempdir()?;
    let root = root.path();
    let coordinator = Coordinator::new(
        "runtime".into(),
        Arc::new(MemoryStorage::new()),
        CoordinatorOptions::default(),
    )
    .await?;
    let mut installs = installs();
    if let Some(codex) = installs.codex.as_mut() {
        codex.adapter.path = root.join("no-codex-acp.js");
    }
    let mut deployment = Deployment::new(root.join("deployment"));
    deployment.openhuman.ready = Duration::from_secs(60);
    let seat = |member: &Member| ProcessSeat::new(member.seat.clone(), root.join(member.id));
    let failed = ProcessSeats::start(
        &coordinator,
        Services::new(coordinator.clone()),
        vec![seat(&first), seat(&broken)],
        &installs,
        &deployment,
    )
    .await;
    ensure!(
        matches!(failed, Err(tinyhivemind_deploy::Error::Start { ref seat, .. }) if seat == "codex"),
        "the broken start: {failed:?}"
    );
    ensure!(
        coordinator.list_agents()?.is_empty(),
        "a failed start left seats registered: {:?}",
        coordinator.list_agents()?
    );
    let mut running = ProcessSeats::start(
        &coordinator,
        Services::new(coordinator.clone()),
        vec![seat(&first)],
        &installs,
        &deployment,
    )
    .await?;
    ensure!(running.seats() == vec!["openhuman"], "{running:?}");
    ensure!(coordinator.list_agents()? == vec!["openhuman".to_owned()]);
    running.stop().await?;
    Ok(())
}
