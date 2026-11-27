//! `live_*`: ACP seats, alone and beside an `OpenHuman` seat, in a real hive.
//! The edit tests are in `hive/edit.rs` and the lead-only episodes in
//! `hive/delegation.rs`.
//!
//! Each seat runs a pinned agent through its runner, is registered with a
//! real coordinator and reaches its hive tools through a real
//! `tinyhivemind-mcp` server, with its own bearer. Only the model is
//! scripted: `hive/model.rs` speaks the three model APIs, reads each
//! rendered turn and calls `hivemind_post`, the same call again (once, then
//! the host's `write_file`, for a seat that edits), then
//! `hivemind_complete`; or, for a lead-only episode, has the lead ask the
//! other harnesses' seats and complete once they have answered.
//!
//! Opt in with `THM_NODE` (a Node 22+ binary) and, per agent,
//! `THM_ACP_CLAUDE` (the `claude-agent-acp` script), `THM_ACP_CODEX` (the
//! `codex-acp` script) with `THM_CODEX` (its Codex binary, for the model
//! catalog), `THM_OPENCLAW` (`OpenClaw`'s `openclaw.mjs`, whose seats share
//! a Gateway the test starts), and `THM_OPENHUMAN_CORE` for the mixed hives.
//! A test whose agents are missing says so on stderr and passes.
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
use tinyhivemind_acp::agent::write_codex_catalog;
use tinyhivemind_acp::openclaw::{Gateway, GatewayPlan, GatewaySeat};
use tinyhivemind_acp::{AcpHarness, Agent};
use tinyhivemind_exec::ExecContext;
use tinyhivemind_hives::{
    AgentRegistration, Coordinator, CoordinatorOptions, Destination, EpisodePhase, HiveInfo,
    MemoryStorage, Message, SendMessage,
};
use tinyhivemind_host::harness::{Approver, Harness, SeatPlan};
use tinyhivemind_lang::{
    Access, ExecTool, LoweredModel, LoweredSeat, LoweredSeatIntegration, Model, Provider,
    ProviderKind,
};
use tinyhivemind_mcp::{HiveToolServer, SeatTools, ServerOptions, Services};

/// The pinned versions, as each agent reports itself.
const CLAUDE_ACP: &str = "0.112.0";
const CODEX_ACP: &str = "2.12.1";
const OPENHUMAN: &str = "0.64.15";
const OPENCLAW: &str = "2026.10.33";
/// Long enough for every seat to start and finish its turns.
const SETTLE: Duration = Duration::from_secs(300);
/// Long enough for an `OpenClaw` Gateway's cold start on a loaded machine.
const GATEWAY_READY: Duration = Duration::from_secs(180);

fn var(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

/// One seat of the hive: its harness, its lowered seat, when it may edit,
/// the access the host's edit tool runs under, and whether its tools go to
/// the hive's `OpenClaw` Gateway.
struct Member {
    id: &'static str,
    harness: Arc<dyn Harness>,
    seat: LoweredSeat,
    edit: Option<Access>,
    gateway: bool,
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

fn claude(id: &'static str, port: u16) -> Option<Member> {
    let agent = Agent::claude_code(var("THM_NODE")?, var("THM_ACP_CLAUDE")?, CLAUDE_ACP);
    let endpoint = format!("http://127.0.0.1:{port}");
    Some(Member {
        id,
        harness: Arc::new(AcpHarness::new(agent)),
        seat: seat(id, ProviderKind::Anthropic, &endpoint),
        edit: None,
        gateway: false,
    })
}

fn codex(id: &'static str, port: u16, root: &Path) -> Result<Option<Member>> {
    let (Some(node), Some(adapter), Some(binary)) =
        (var("THM_NODE"), var("THM_ACP_CODEX"), var("THM_CODEX"))
    else {
        return Ok(None);
    };
    let catalog = root.join(format!("{id}-catalog.json"));
    write_codex_catalog(&binary, &catalog, "mock")?;
    let agent = Agent::codex(node, adapter, CODEX_ACP, &catalog);
    let endpoint = format!("http://127.0.0.1:{port}/v1");
    Ok(Some(Member {
        id,
        harness: Arc::new(AcpHarness::new(agent)),
        seat: seat(id, ProviderKind::Openai, &endpoint),
        edit: None,
        gateway: false,
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
        edit: None,
        gateway: false,
    })
}

/// The Gateway a hive's `OpenClaw` seats share, in `root`, on the model at
/// `port`; its seats are added when the hive starts.
fn gateway(port: u16, root: &Path) -> Result<Option<GatewayPlan>> {
    let (Some(node), Some(openclaw)) = (var("THM_NODE"), var("THM_OPENCLAW")) else {
        return Ok(None);
    };
    let free = std::net::TcpListener::bind("127.0.0.1:0")?;
    let gateway_port = free.local_addr()?.port();
    drop(free);
    Ok(Some(GatewayPlan {
        node,
        openclaw,
        directory: root.join("gateway"),
        port: gateway_port,
        token: format!("thm-gateway-{gateway_port}"),
        endpoint: format!("http://127.0.0.1:{port}/v1"),
        model: "mock".into(),
        seats: Vec::new(),
    }))
}

/// Seat `id` on `OpenClaw`, through the Gateway `gateway` plans.
fn openclaw(id: &'static str, port: u16, gateway: &GatewayPlan) -> Member {
    let agent = Agent::openclaw(
        gateway.node.clone(),
        gateway.openclaw.clone(),
        OPENCLAW,
        gateway,
    );
    let endpoint = format!("http://127.0.0.1:{port}/v1");
    Member {
        id,
        harness: Arc::new(AcpHarness::new(agent)),
        seat: seat(id, ProviderKind::Openai, &endpoint),
        edit: None,
        gateway: true,
    }
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

/// How a hive runs: its coordinator's options, the host's approver for
/// supervised seats' edits, and the Gateway its `OpenClaw` seats share.
#[derive(Default)]
struct Setup {
    options: CoordinatorOptions,
    approver: Option<Arc<dyn Approver>>,
    gateway: Option<GatewayPlan>,
}

/// A started hive.
struct Running {
    coordinator: Coordinator,
    server: HiveToolServer,
    gateway: Option<Gateway>,
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
    stop_hive(&members, running).await
}

/// A coordinator and hive tool server set up as `setup` says, with every
/// member's tools served; then the `OpenClaw` Gateway, given the tools of
/// the seats on it; then every member started, registered and in hive
/// `work`.
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
    let server = HiveToolServer::start(services, ServerOptions::default()).await?;
    let mut endpoints = Vec::new();
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
        endpoints.push(server.serve_seat(tools)?);
    }
    let gateway = match setup.gateway {
        Some(mut plan) => {
            plan.seats = members
                .iter()
                .zip(&endpoints)
                .filter(|(member, _)| member.gateway)
                .map(|(member, tools)| GatewaySeat {
                    id: member.id.into(),
                    tools: tools.clone(),
                })
                .collect();
            let gateway = Gateway::start(&plan, GATEWAY_READY).await;
            Some(gateway.context("starting the OpenClaw gateway")?)
        }
        None => None,
    };
    for (member, tools) in members.iter().zip(endpoints) {
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
            members: members.iter().map(|member| member.id.into()).collect(),
        })
        .await?;
    Ok(Running {
        coordinator,
        server,
        gateway,
    })
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

/// Stop every member's seat, then the Gateway, then the tool server.
async fn stop_hive(members: &[Member], running: Running) -> Result<()> {
    for member in members {
        member.harness.stop(member.id).await?;
    }
    if let Some(gateway) = &running.gateway {
        gateway.stop().await?;
    }
    running.server.shutdown().await?;
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
    let root = tempfile::tempdir()?;
    let Some(member) = codex("codex", model.port, root.path())? else {
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
        claude("claude", model.port),
        codex("codex", model.port, root.path())?,
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

#[tokio::test(flavor = "multi_thread")]
async fn live_an_openclaw_seat_posts_once_and_settles() -> Result<()> {
    let model = HiveModel::start().await?;
    let root = tempfile::tempdir()?;
    let Some(gateway) = gateway(model.port, root.path())? else {
        eprintln!("skipped: set THM_NODE and THM_OPENCLAW");
        return Ok(());
    };
    let member = openclaw("openclaw", model.port, &gateway);
    let setup = Setup {
        gateway: Some(gateway),
        ..Setup::default()
    };
    hive_with(vec![member], &model, root.path(), setup).await
}

/// One episode, four harnesses: `OpenHuman` over its own API, and Claude
/// Code, Codex and `OpenClaw` over ACP, each seat posting once as itself.
#[tokio::test(flavor = "multi_thread")]
async fn live_a_hive_of_all_four_harnesses_settles() -> Result<()> {
    let model = HiveModel::start().await?;
    let root = tempfile::tempdir()?;
    let (Some(openhuman), Some(claude), Some(codex), Some(gateway)) = (
        openhuman(model.port),
        claude("claude", model.port),
        codex("codex", model.port, root.path())?,
        gateway(model.port, root.path())?,
    ) else {
        eprintln!(
            "skipped: set THM_OPENHUMAN_CORE, THM_NODE, THM_ACP_CLAUDE, THM_ACP_CODEX, THM_CODEX and THM_OPENCLAW"
        );
        return Ok(());
    };
    let openclaw = openclaw("openclaw", model.port, &gateway);
    let setup = Setup {
        gateway: Some(gateway),
        ..Setup::default()
    };
    let members = vec![openhuman, claude, codex, openclaw];
    hive_with(members, &model, root.path(), setup).await
}
