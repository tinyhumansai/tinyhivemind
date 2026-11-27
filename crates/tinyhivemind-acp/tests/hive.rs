//! `live_*`: ACP seats, alone and beside an `OpenHuman` seat, in a real hive.
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
//! catalog), and `THM_OPENHUMAN_CORE` for the mixed hive. A test whose
//! agents are missing says so on stderr and passes.
#![cfg(target_os = "linux")]

#[path = "hive/model.rs"]
mod model;

#[path = "hive/delegation.rs"]
mod delegation;

use anyhow::{Context as _, Result, ensure};
use model::{EDIT_TOOL, EDITS, HiveModel, Seen, edited};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tinyhivemind_acp::agent::write_codex_catalog;
use tinyhivemind_acp::{AcpHarness, Agent};
use tinyhivemind_exec::ExecContext;
use tinyhivemind_hives::{
    AgentRegistration, Coordinator, CoordinatorOptions, Destination, EpisodePhase, HiveInfo,
    MemoryStorage, Message, SendMessage,
};
use tinyhivemind_host::harness::{
    ApprovalFuture, Approver, Harness, SeatPlan, ToolApproval, ToolDecision,
};
use tinyhivemind_lang::{
    Access, ExecTool, LoweredModel, LoweredSeat, LoweredSeatIntegration, Model, Provider,
    ProviderKind,
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

/// One seat of the hive: its harness, its lowered seat and, when it may
/// edit, the access the host's edit tool runs under.
struct Member {
    id: &'static str,
    harness: Arc<dyn Harness>,
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

/// What the host's approver tells a seat it denies.
const DENIED: &str = "this host denies seat refused";

/// The host's approver for supervised seats: it records every request and
/// allows all but seat `refused`'s.
#[derive(Default)]
struct Approvals {
    asked: Mutex<Vec<ToolApproval>>,
}

impl Approvals {
    fn asked(&self) -> Vec<ToolApproval> {
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Approver for Approvals {
    fn decide(&self, request: ToolApproval) -> ApprovalFuture {
        let denied = request.seat == "refused";
        self.asked
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(request);
        Box::pin(async move {
            if denied {
                ToolDecision::Deny {
                    reason: DENIED.into(),
                }
            } else {
                ToolDecision::Allow
            }
        })
    }
}

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
    })
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

/// Run one episode with every member as a starter, and check each seat and
/// the episode; then stop every seat.
async fn hive(members: Vec<Member>, model: &HiveModel, root: &Path) -> Result<()> {
    hive_with(members, model, root, None).await
}

/// [`hive`], with `approver` asked about supervised seats' edits.
async fn hive_with(
    members: Vec<Member>,
    model: &HiveModel,
    root: &Path,
    approver: Option<Arc<dyn Approver>>,
) -> Result<()> {
    let options = CoordinatorOptions::default();
    let (coordinator, server) = start_hive(&members, root, options, approver).await?;
    let ids = members.iter().map(|member| member.id.to_owned()).collect();
    settle(&coordinator, ids).await?;
    let transcript = coordinator.read_transcript(None)?;
    let seen = model.seen();
    for member in &members {
        seat_took_its_turn(member.id, member.edit.is_some(), &transcript, &seen)?;
    }
    stop_hive(&members, server).await
}

/// A coordinator under `options` and a hive tool server, with every member
/// started, registered and in hive `work`, `approver` asked about supervised
/// seats' edits.
async fn start_hive(
    members: &[Member],
    root: &Path,
    options: CoordinatorOptions,
    approver: Option<Arc<dyn Approver>>,
) -> Result<(Coordinator, HiveToolServer)> {
    let coordinator =
        Coordinator::new("runtime".into(), Arc::new(MemoryStorage::new()), options).await?;
    let mut services = Services::new(coordinator.clone());
    if let Some(approver) = approver {
        services = services.with_approver(approver);
    }
    let server = HiveToolServer::start(services, ServerOptions::default()).await?;
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
        let tools = server.serve_seat(tools)?;
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
    Ok((coordinator, server))
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

/// Stop every member's seat, then the tool server.
async fn stop_hive(members: &[Member], server: HiveToolServer) -> Result<()> {
    for member in members {
        member.harness.stop(member.id).await?;
    }
    server.shutdown().await?;
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

/// The results of seat `id`'s edits, in [`EDITS`] order: those its last
/// model call carried.
fn edit_results(model: &HiveModel, id: &str) -> Result<Vec<String>> {
    let last = model
        .seen()
        .into_iter()
        .filter(|seen| seen.agent == id)
        .max_by_key(|seen| seen.results.len())
        .with_context(|| format!("the model never saw a turn of {id}"))?;
    let edits = last.results.get(1..=EDITS.len()).map(<[String]>::to_vec);
    edits.with_context(|| format!("{id} did not make every edit: {:?}", last.results))
}

/// Every harness edits only through the host's tool, and the host judges
/// each edit. An autonomous `OpenHuman` seat and supervised Claude Code and
/// Codex seats, which the host's approver allows, each write their own file;
/// a read-only Claude Code seat and a supervised Codex seat the approver
/// denies write nothing. Every editing seat also retries its write, which
/// the host answers with the first result without running it again, and
/// tries a path that climbs out of its workspace and an absolute one, which
/// the tool refuses.
#[tokio::test(flavor = "multi_thread")]
async fn live_seats_on_every_harness_edit_only_through_the_host() -> Result<()> {
    let root = tempfile::tempdir()?;
    let root = root.path();
    let outside = root.join("outside");
    std::fs::create_dir_all(&outside)?;
    let model = HiveModel::start_with(&outside.to_string_lossy()).await?;
    let (Some(openhuman), Some(claude), Some(codex), Some(reader), Some(refused)) = (
        openhuman(model.port),
        claude("claude", model.port),
        codex("codex", model.port, root)?,
        claude("reader", model.port),
        codex("refused", model.port, root)?,
    ) else {
        eprintln!(
            "skipped: set THM_OPENHUMAN_CORE, THM_NODE, THM_ACP_CLAUDE, THM_ACP_CODEX and THM_CODEX"
        );
        return Ok(());
    };
    let members = vec![
        openhuman.editing(Access::Autonomous),
        claude.editing(Access::Supervised),
        codex.editing(Access::Supervised),
        reader.editing(Access::ReadOnly),
        refused.editing(Access::Supervised),
    ];
    let approvals = Arc::new(Approvals::default());
    hive_with(members, &model, root, Some(approvals.clone())).await?;
    let log = std::fs::read_to_string(root.join("writes.log")).unwrap_or_default();
    let runs = |id: &str| {
        log.lines()
            .filter(|line| line.split(' ').next() == Some(id))
            .count()
    };

    for id in ["openhuman", "claude", "codex"] {
        let file = workspace(root, id).join(format!("{id}.md"));
        let written = std::fs::read_to_string(&file)
            .with_context(|| format!("{id} did not write {}", file.display()))?;
        ensure!(written == edited(id), "{id} wrote {written:?}");
        let entries = std::fs::read_dir(workspace(root, id))?.count();
        ensure!(entries == 1, "{id}'s workspace holds {entries} entries");
        // The host ran the program once, for the edit, and nothing else.
        ensure!(
            runs(id) == 1,
            "the host wrote {} times for {id}: {log:?}",
            runs(id)
        );
        let results = edit_results(&model, id)?;
        ensure!(
            results[0].contains(&format!("wrote {id}.md")),
            "{id}'s edit did not come back from the host's tool: {:?}",
            results[0]
        );
        ensure!(
            results[1] == results[0],
            "{id}'s retry was not answered with the first result: {results:?}"
        );
        for (edit, result) in EDITS.iter().zip(&results).skip(2) {
            ensure!(
                result.contains("outside the workspace"),
                "{id}'s {edit} write was not refused: {result:?}"
            );
        }
    }

    for (id, reason) in [("reader", "read_only"), ("refused", DENIED)] {
        let entries = std::fs::read_dir(workspace(root, id))?.count();
        ensure!(entries == 0, "{id} wrote {entries} entries");
        ensure!(runs(id) == 0, "the host ran {id}'s write: {log:?}");
        let results = edit_results(&model, id)?;
        ensure!(
            results.iter().all(|result| result.contains(reason)),
            "{id} was not told every write was refused ({reason}): {results:?}"
        );
    }

    // Nothing landed outside a workspace: no escape file beside them, and
    // nothing in the directory the absolute writes aimed at.
    let beside: BTreeSet<String> = std::fs::read_dir(root.join("workspaces"))?
        .map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned()))
        .collect::<std::io::Result<_>>()?;
    let seats: BTreeSet<String> = ["openhuman", "claude", "codex", "reader", "refused"]
        .map(str::to_owned)
        .into();
    ensure!(beside == seats, "the workspaces directory holds {beside:?}");
    let outside_entries = std::fs::read_dir(&outside)?.count();
    ensure!(
        outside_entries == 0,
        "{outside_entries} files landed outside"
    );

    // The approver judged exactly the supervised seats, and the edit's
    // retry was answered from the host's record without asking it again.
    let asked = approvals.asked();
    let judged: BTreeSet<&str> = asked.iter().map(|request| request.seat.as_str()).collect();
    ensure!(
        judged == BTreeSet::from(["claude", "codex", "refused"]),
        "the approver judged {judged:?}"
    );
    for id in ["claude", "codex"] {
        let edits = asked
            .iter()
            .filter(|request| request.seat == id && request.call_id == "edit")
            .count();
        ensure!(
            edits == 1,
            "the approver was asked {edits} times about {id}'s edit"
        );
    }
    Ok(())
}
