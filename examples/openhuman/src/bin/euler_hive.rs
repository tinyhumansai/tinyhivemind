//! A hive of OpenHuman agents, defined entirely by the `euler-hive`
//! hive-language package, attempts a problem it is handed at run time.
//!
//! The package is loaded from disk, its placeholder problem replaced by the
//! statement passed with `--problem`, then parsed and lowered. Every lowered
//! seat becomes an OpenHuman agent through the language adapter's
//! `management_request` and `memory_binding`, bound to CortexDB behind the
//! TinyHumans backend. Each desk runs on its own floor under core's
//! completion driver; desks reach each other only by core's referral fold.
//! Inference is OpenRouter; web search (research seats only) is the TinyHumans
//! backend's Exa and Gemini integrations through the TinyHumans SDK; code runs
//! (compute and verify seats only) in a network-less Docker sandbox with
//! nothing mounted. See `euler_hive/README.md`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures::future::join_all;
use openhuman_embed::{Provider, Runtime};
use serde_json::json;
use tinyhivemind_lang::MemoryKind;
use tinyhivemind_openhuman::{HiveMemory, ManagementRequest, language};
use tinymemory_api::MemoryEngine;
use tinymemory_integrations::cortex::{CortexEngine, StaticBearer};

#[path = "euler_hive/agents.rs"]
mod agents;
#[path = "euler_hive/artifacts.rs"]
mod artifacts;
#[path = "euler_hive/budget.rs"]
mod budget;
#[path = "euler_hive/floors.rs"]
mod floors;
#[path = "euler_hive/frames.rs"]
mod frames;
#[path = "euler_hive/memory.rs"]
mod memory;
#[path = "euler_hive/package.rs"]
mod package;
#[path = "euler_hive/referrals.rs"]
mod referrals;
#[path = "euler_hive/report.rs"]
mod report;
#[path = "euler_hive/router.rs"]
mod router;
#[path = "euler_hive/sandbox.rs"]
mod sandbox;
#[path = "euler_hive/search.rs"]
mod search;
#[path = "euler_hive/tools.rs"]
mod tools;
#[path = "euler_hive/turn.rs"]
mod turn;

use budget::{Budget, Limits};
use frames::{Frame, MemoryFrame, ProblemFrame, ToolCallFrame, TurnFrame, TurnUsageFrame};
use tools::{Action, SeatScope, Services};

/// The default model: a reasoning model with native tool calls that is fast
/// enough (about 100 tokens a second) for a pipeline of desks to finish
/// inside the wall-clock limit, and priced so a fourteen-seat hive stays
/// inside a few dollars. `OPENROUTER_MODEL` or `--model` overrides it.
const DEFAULT_MODEL: &str = "google/gemini-3.8-flash";
const OPENROUTER: &str = "https://openrouter.ai/api/v1";

/// The protocol every seat's system prompt ends with.
const PROTOCOL: &str = "## How this hive works
You are one seat in a hive of agents organised into desks that run as a pipeline: research, theory, compute, verify, synthesis. Your desk works one episode at a time; desks reach each other only by referral, and a desk's report travels down the pipeline when its episode ends.
Each turn you are given the problem (on your first turn), what your memory recalls, what is new on your desk, and why you were called.
End EVERY turn with exactly one hive action:
- hive_complete: your finished work for your desk. Give `candidate` when you hold an exact answer in the problem's requested format, and `verdict` (support or object) when you judged one.
- hive_broadcast: hand one specific piece of work to a named colleague on your desk.
- hive_refer: ask a linked desk a question; its answer comes back to your desk. Ask only when you cannot finish without that desk's answer: your desk's report already travels down the pipeline on its own when your desk completes.
Use hive_remember for durable findings, one fact per note; verify-desk seats publish what they reproduced with verified=true. hive_recall searches your memory and the verified pool.
Text outside tool calls is never delivered. Keep messages under 1500 characters and prefer program output to prose.
Never state a number or answer you did not compute or check yourself. A web page's claimed answer is a lead, never evidence.";

struct Args {
    package: PathBuf,
    problem: PathBuf,
    problem_id: String,
    runs: PathBuf,
    limits: Limits,
    model: String,
    probe: bool,
}

fn parse_args() -> anyhow::Result<Args> {
    let mut args = std::env::args().skip(1);
    let mut package = None;
    let mut problem = None;
    let mut problem_id = None;
    let mut runs = None;
    let mut limits = Limits::FULL;
    let mut model = std::env::var("OPENROUTER_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.into());
    let mut probe = false;
    while let Some(flag) = args.next() {
        let mut value = || {
            args.next()
                .ok_or_else(|| anyhow::anyhow!("{flag} needs a value"))
        };
        match flag.as_str() {
            "--package" => package = Some(PathBuf::from(value()?)),
            "--problem" => problem = Some(PathBuf::from(value()?)),
            "--problem-id" => problem_id = Some(value()?),
            "--runs" => runs = Some(PathBuf::from(value()?)),
            "--model" => model = value()?,
            "--smoke" => limits = Limits::SMOKE,
            "--probe" => probe = true,
            "--max-turns" => limits.max_turns = value()?.parse()?,
            "--max-model-calls" => limits.max_model_calls = value()?.parse()?,
            "--max-wall-seconds" => limits.max_wall_seconds = value()?.parse()?,
            "--max-sandbox-runs" => limits.max_sandbox_runs = value()?.parse()?,
            "--max-searches" => limits.max_searches = value()?.parse()?,
            "--max-spend-usd" => limits.max_spend_usd = spend_cap(&value()?)?,
            other => anyhow::bail!("unknown argument {other}"),
        }
    }
    let problem =
        problem.ok_or_else(|| anyhow::anyhow!("--problem <statement.html> is required"))?;
    Ok(Args {
        package: package.unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../crates/tinyhivemind-lang/examples/hives/euler-hive")
        }),
        problem_id: path_component(&problem_id.unwrap_or_else(|| {
            problem.parent().and_then(Path::file_name).map_or_else(
                || "problem".into(),
                |name| name.to_string_lossy().into_owned(),
            )
        }))?,
        runs: runs
            .ok_or_else(|| anyhow::anyhow!("--runs <dir outside the repository> is required"))?,
        problem,
        limits,
        model,
        probe,
    })
}

/// A spend cap: a finite, non-negative number of dollars. `NaN` or infinity
/// would make every `spend >= cap` comparison false and disable the guard.
fn spend_cap(value: &str) -> anyhow::Result<f64> {
    let cap: f64 = value.parse()?;
    if !cap.is_finite() || cap < 0.0 {
        anyhow::bail!("--max-spend-usd must be a finite, non-negative number of dollars");
    }
    Ok(cap)
}

/// A problem id as one path component of `[A-Za-z0-9_-]`, so the run
/// directory cannot leave `--runs`.
fn path_component(id: &str) -> anyhow::Result<String> {
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        anyhow::bail!("--problem-id must be letters, digits, '-' or '_' (got {id:?})");
    }
    Ok(id.to_owned())
}

fn secret(name: &str) -> anyhow::Result<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("{name} is not set"))
}

fn main() -> anyhow::Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(16 * 1024 * 1024)
        .build()?
        .block_on(async { tokio::spawn(run(parse_args()?)).await? })
}

/// Open the first desk after the recording's initial artifacts were written.
fn open_federation(
    federation: &mut floors::Federation,
    run: &artifacts::RunDir,
) -> anyhow::Result<()> {
    run.check()?;
    federation.open("The host opened the hive on the problem above. Work it from your desk's angle, then report with hive_complete.");
    Ok(())
}

async fn run(args: Args) -> anyhow::Result<()> {
    // Provider-side browsing would bypass the research-only search belt.
    let model = args.model.to_ascii_lowercase();
    if ["online", "search", "sonar", "web"]
        .iter()
        .any(|word| model.contains(word))
    {
        anyhow::bail!("web-enabled model ids are refused; research uses the filtered search tool");
    }
    let openrouter_key = secret("OPENROUTER_API_KEY")?;
    let tinyhumans_key = secret("TINYHUMANS_API_KEY")?;
    let backend =
        std::env::var("TINYHUMANS_BACKEND_URL").unwrap_or_else(|_| search::DEFAULT_BACKEND.into());
    let statement = package::problem_markdown(&std::fs::read_to_string(&args.problem)?);
    let documents = package::read_package(&args.package)?;
    let (package_doc, lowered) = package::load(documents, &statement)?;
    let run_id = format!("{}-{}", artifacts::timestamp(), artifacts::unique_suffix());
    let run = Arc::new(artifacts::RunDir::create(
        args.runs.join(&args.problem_id).join(&run_id),
    )?);
    println!("run_dir: {}", run.path().display());
    println!("model: {}", args.model);
    report::structure(&args.package, &run);

    let engine: Arc<dyn MemoryEngine> = Arc::new(CortexEngine::tinyhumans(
        &backend,
        Arc::new(StaticBearer::new(tinyhumans_key.clone())),
    )?);
    let health = engine.health().await;
    println!("memory_engine: cortexdb via tinyhumans ({health:?})");
    let sandbox = sandbox::PythonSandbox::from_env();
    sandbox.preflight()?;
    println!(
        "sandbox: docker {} (no network, nothing mounted)",
        sandbox.image()
    );
    let search = search::WebSearch::new(
        &backend,
        tinyhumans_key.clone(),
        std::env::var("EULER_HIVE_GEMINI_MODEL")
            .unwrap_or_else(|_| search::DEFAULT_GEMINI_MODEL.into()),
    );
    if args.probe {
        return report::probe(
            &search,
            &sandbox,
            engine.as_ref(),
            &run,
            &lowered.memory.root,
            &run_id,
        )
        .await;
    }

    let budget = Arc::new(Budget::new(args.limits));
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()?;
    match budget
        .poll_usage(budget::openrouter_usage(&http, &openrouter_key))
        .await
    {
        Some(Ok(usage)) => {
            budget.record_account(usage);
            println!("openrouter_usage: metered");
        }
        Some(Err(error)) => println!("openrouter_usage: unavailable ({error})"),
        None => println!("openrouter_usage: skipped at wall-clock deadline"),
    }

    // One runtime for every seat: OpenRouter inference, the real TinyHumans
    // backend, and CortexDB as the process memory engine.
    let mut config = tinyhivemind_openhuman::offline::config();
    HiveMemory::with_root(lowered.memory.root.clone())?.configure(&mut config);
    config.memory.conversations.enabled = true;
    config.memory.recall.enabled = false;
    config.agent.compact_context = false;
    config.default_temperature = 0.2;
    let runtime = Arc::new(
        Box::pin(
            Runtime::builder()
                .config(config)
                .workspace_dir(run.path().join("openhuman"))
                .backend_url(backend.clone())
                .provider(
                    Provider::openai_compatible(OPENROUTER, openrouter_key.clone())
                        .model(&args.model),
                )
                .memory_engine(engine.clone())
                .build(),
        )
        .await?,
    );

    println!("runtime: built");
    let federation_seed = floors::Federation::new(&lowered)?;
    let charters: BTreeMap<String, String> = lowered
        .candidates
        .iter()
        .map(|candidate| {
            (
                candidate.id.clone(),
                candidate.description.clone().unwrap_or_default(),
            )
        })
        .collect();
    let factory = agents::SeatFactory {
        runtime: runtime.clone(),
        charters,
        protocol: PROTOCOL.into(),
        package_root: lowered.memory.root.clone(),
        run: run_id.clone(),
    };
    let pool_identity = lowered
        .memory
        .identities
        .iter()
        .find(|identity| identity.kind == MemoryKind::Pool);
    let pool = pool_identity
        .map(|identity| memory::run_scoped(&identity.namespace, &lowered.memory.root, &run_id))
        .transpose()?;
    let pool_read_only = pool_identity.is_some_and(|identity| {
        identity.read_only
            || package_doc
                .constitution
                .read_only_memories
                .contains(&identity.id)
    });
    let services = Arc::new(Services {
        search: Some(search),
        sandbox,
        budget: budget.clone(),
        run: run.clone(),
        run_id: run_id.clone(),
        literals: search::statement_literals(&statement),
    });
    let mut seats: BTreeMap<String, Arc<turn::Seat>> = BTreeMap::new();
    for seat in &lowered.seats {
        let belt = package::seat_tools(seat)?;
        let request = language::management_request(seat);
        let ManagementRequest::CreateAgent {
            template,
            config,
            memory: binding,
            ..
        } = request
        else {
            anyhow::bail!("the adapter produced no CreateAgent for {}", seat.id);
        };
        let binding = binding.map(|binding| *binding);
        let agent = tinyhivemind_openhuman::AgentFactory::create_with_memory(
            &factory,
            template,
            config,
            binding.clone(),
        )
        .await?;
        let lowered_binding =
            binding.ok_or_else(|| anyhow::anyhow!("{} has no memory", seat.id))?;
        let scoped = memory::scope_binding(&lowered_binding, &lowered.memory.root, &run_id)?;
        let installed = &agent.config().memory;
        if installed.agent_id.as_deref() != Some(scoped.agent_id.as_str())
            || installed.root.as_deref() != Some(scoped.root.as_str())
        {
            anyhow::bail!("{} was not bound to its memory namespace", seat.id);
        }
        let desk = package::desk_of(&lowered, &seat.id)
            .ok_or_else(|| anyhow::anyhow!("{} sits on no desk", seat.id))?;
        let pool_writer = desk.id == "verify";
        let seat_memory = Arc::new(
            memory::SeatMemory::new(engine.clone(), scoped, pool.clone(), pool_writer)
                .read_only_pool(pool_read_only),
        );
        let scope = SeatScope {
            seat: seat.id.clone(),
            desk: desk.id.clone(),
            colleagues: desk
                .members
                .iter()
                .filter(|m| **m != seat.id)
                .cloned()
                .collect(),
            linked_desks: federation_seed.linked_from(&desk.id),
            conversation: format!("euler-{run_id}"),
        };
        println!(
            "seat {} on #{} tools=[{}] memory={}",
            seat.id,
            desk.id,
            belt.iter()
                .map(|tool| tool.name())
                .collect::<Vec<_>>()
                .join(","),
            seat_memory.binding().namespace
        );
        seats.insert(
            seat.id.clone(),
            Arc::new(turn::Seat::new(agent, belt, seat_memory, scope)),
        );
    }
    run.json("package.json", &json!({
        "id": lowered.id,
        "seats": lowered.seats.iter().map(|seat| json!({"id": seat.id, "tools": seat.integration.tools, "memory": seat.memory})).collect::<Vec<_>>(),
        "desks": lowered.desks,
        "topology": lowered.topology,
        "referrals": lowered.referrals,
        "policies": {"referral": lowered.policies.referral, "routing": lowered.policies.routing},
        "constitution": package_doc.constitution,
    }));
    run.frame(&Frame::Start {
        package: lowered.id.clone(),
        seed: 0,
        seats: lowered.seats.len(),
        problem: Some(ProblemFrame {
            id: args.problem_id.clone(),
            statement: statement.clone(),
            answer: None,
        }),
        live: Some(true),
        model: Some(args.model.clone()),
        limits: serde_json::to_value(args.limits)?,
    });

    let mut federation = federation_seed;
    open_federation(&mut federation, &run)?;
    let mut stop = None;
    loop {
        if federation.finished.is_some() || federation.undecided.is_some() {
            break;
        }
        let pending = federation.ready()?;
        if pending.is_empty() {
            stop = Some("idle".to_owned());
            break;
        }
        let width: u64 = pending.iter().map(|round| round.seats.len() as u64).sum();
        if let Some(Ok(usage)) = budget
            .poll_usage(budget::openrouter_usage(&http, &openrouter_key))
            .await
        {
            budget.record_account(usage);
        }
        if let Err(exhausted) = budget.admit_round(width) {
            stop = Some(format!(
                "exhausted_{}",
                serde_json::to_value(&exhausted)?
                    .as_str()
                    .unwrap_or("budget")
            ));
            break;
        }
        budget
            .turns
            .fetch_add(width, std::sync::atomic::Ordering::Relaxed);
        let budget_line = format!(
            "{} seat turns, {} sandbox runs and {} web searches remain for the whole hive.",
            args.limits
                .max_turns
                .saturating_sub(budget.turns.load(std::sync::atomic::Ordering::Relaxed)),
            args.limits.max_sandbox_runs.saturating_sub(
                budget
                    .sandbox_runs
                    .load(std::sync::atomic::Ordering::Relaxed)
            ),
            args.limits
                .max_searches
                .saturating_sub(budget.searches.load(std::sync::atomic::Ordering::Relaxed)),
        );
        let mut launched = Vec::new();
        for round in &pending {
            let floor = &federation.floors[round.desk];
            let desk_line = format!(
                "#{} ({}): {}. Colleagues: {}. You may refer to: {}.",
                floor.desk.id,
                floor.desk.name,
                floor.desk.description.clone().unwrap_or_default(),
                floor
                    .desk
                    .members
                    .iter()
                    .map(|m| format!("@{m}"))
                    .collect::<Vec<_>>()
                    .join(", "),
                federation
                    .linked_from(&floor.desk.id)
                    .iter()
                    .map(|d| format!("#{d}"))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            for id in &round.seats {
                let (rows, saw, of) = federation.delta(round.desk, id);
                let assignment = federation.assignment(round.desk, id);
                println!("[turn] @{id} on #{} ({})", floor.desk.id, assignment.reason);
                let seat = seats
                    .get(id)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("no agent for {id}"))?;
                let input = turn::TurnInput {
                    problem: statement.clone(),
                    desk_line: desk_line.clone(),
                    rows,
                    assignment: assignment.clone(),
                    budget_line: budget_line.clone(),
                };
                launched.push((
                    round.desk,
                    saw,
                    of,
                    assignment.reason,
                    turn::run(seat, input, services.clone()),
                ));
            }
        }
        let shapes: Vec<_> = launched
            .iter()
            .map(|(desk, saw, of, reason, _)| (*desk, *saw, *of, reason.clone()))
            .collect();
        let outputs = join_all(launched.into_iter().map(|(.., future)| future)).await;
        let mut by_desk: BTreeMap<usize, Vec<(turn::TurnOutput, usize, usize, String)>> =
            BTreeMap::new();
        for ((desk, saw, of, reason), output) in shapes.into_iter().zip(outputs) {
            report::turn_artifacts(&run, &output);
            by_desk
                .entry(desk)
                .or_default()
                .push((output, saw, of, reason));
        }
        for round in &pending {
            let Some(outputs) = by_desk.remove(&round.desk) else {
                continue;
            };
            let journal_at_start = outputs.first().map_or(0, |(_, _, of, _)| *of);
            let results: Vec<(String, Action)> = outputs
                .iter()
                .map(|(output, ..)| (output.seat.clone(), output.action.clone()))
                .collect();
            let referrals_before = federation.referrals.len();
            let sequences = federation
                .commit(round.desk, journal_at_start, results)
                .await?;
            let floor = &federation.floors[round.desk];
            let turns = outputs
                .iter()
                .zip(sequences)
                .map(|((output, saw, of, reason), sequence)| {
                    turn_frame(output, &floor.desk.id, reason, *saw, *of, sequence)
                })
                .collect();
            run.frame(&Frame::Round {
                round: federation.round,
                desk: floor.desk.id.clone(),
                desk_round: floor.rounds,
                phase: round.phase.into(),
                visibility: if round.blind { "blind" } else { "full" }.into(),
                turns,
                standings: federation.standings(round.desk),
            });
            for frame in &federation.referrals[referrals_before..] {
                println!(
                    "[referral] {} #{} -> #{} {}",
                    frame.kind,
                    frame.from_desk,
                    frame.to_desk,
                    if frame.accepted {
                        "accepted".to_owned()
                    } else {
                        format!("refused: {}", frame.reason.clone().unwrap_or_default())
                    }
                );
                run.frame(&Frame::Referral(frame.clone()));
            }
        }
        run.frame(&Frame::Usage(report::usage(&budget)));
        // A record that can no longer be written stops the spend.
        run.check()?;
    }
    if let Some(Ok(usage)) = budget
        .poll_usage(budget::openrouter_usage(&http, &openrouter_key))
        .await
    {
        budget.record_account(usage);
    }
    report::finish(report::Closing {
        run: &run,
        federation: &federation,
        seats: &seats,
        budget: &budget,
        engine: engine.as_ref(),
        pool: pool.as_deref(),
        stop,
        model: &args.model,
    })
    .await?;
    // Give OpenHuman's background conversation logging a moment to land.
    tokio::time::sleep(Duration::from_secs(2)).await;
    Ok(())
}

fn turn_frame(
    output: &turn::TurnOutput,
    desk: &str,
    reason: &str,
    saw: usize,
    of: usize,
    sequence: u64,
) -> TurnFrame {
    let (content, answer) = match &output.action {
        Action::Complete {
            report, candidate, ..
        } => (report.clone(), candidate.clone()),
        Action::Broadcast { to, message } => (format!("@{to} {message}"), None),
        Action::Refer { desk, message } => (format!("@#{desk} {message}"), None),
    };
    TurnFrame {
        seat: output.seat.clone(),
        desk: desk.into(),
        reason: if output.recovered {
            "recovered".into()
        } else {
            reason.into()
        },
        saw,
        of,
        content,
        answer,
        correct: None,
        sequence,
        tool_calls: output
            .calls
            .iter()
            .map(|call| ToolCallFrame {
                tool: call.tool.clone(),
                summary: call.summary.clone(),
                ok: call.ok,
            })
            .collect(),
        memory: MemoryFrame {
            recalled: output.recalled,
            remembered: output.remembered,
        },
        usage: Some(TurnUsageFrame {
            model_calls: output.model_calls,
            input_tokens: output.input_tokens,
            output_tokens: output.output_tokens,
            cost_usd: output.cost_usd,
        }),
    }
}

#[cfg(test)]
#[path = "euler_hive/request_test.rs"]
mod request_test;
#[cfg(test)]
#[path = "euler_hive/test/mod.rs"]
mod test;
