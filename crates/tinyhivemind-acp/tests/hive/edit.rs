//! `live_*`: every harness edits only through the host's `write_file`
//! tool, each seat in its own workspace, and the host judges each edit.

use super::{Setup, claude, codex, gateway, hive_with, openclaw, openhuman, workspace};
use crate::model::{EDITS, HiveModel, edited};
use anyhow::{Context as _, Result, ensure};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use tinyhivemind_host::harness::{ApprovalFuture, Approver, ToolApproval, ToolDecision};
use tinyhivemind_lang::Access;

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
    edits(false).await
}

/// The same, with a supervised `OpenClaw` seat writing its own file beside
/// the rest, its tools reached through the Gateway.
#[tokio::test(flavor = "multi_thread")]
async fn live_an_openclaw_seat_edits_only_through_the_host_beside_the_rest() -> Result<()> {
    edits(true).await
}

/// The edit hive, with an `OpenClaw` writer when `with_openclaw`.
async fn edits(with_openclaw: bool) -> Result<()> {
    let root = tempfile::tempdir()?;
    let root = root.path();
    let outside = root.join("outside");
    std::fs::create_dir_all(&outside)?;
    let model = HiveModel::start_with(&outside.to_string_lossy()).await?;
    let gateway = if with_openclaw {
        gateway(model.port, root)?
    } else {
        None
    };
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
    if with_openclaw && gateway.is_none() {
        eprintln!("skipped: set THM_OPENCLAW too");
        return Ok(());
    }
    let mut members = vec![
        openhuman.editing(Access::Autonomous),
        claude.editing(Access::Supervised),
        codex.editing(Access::Supervised),
        reader.editing(Access::ReadOnly),
        refused.editing(Access::Supervised),
    ];
    let mut writers = vec!["openhuman", "claude", "codex"];
    let mut supervised = vec!["claude", "codex"];
    if let Some(gateway) = &gateway {
        members.push(openclaw("openclaw", model.port, gateway).editing(Access::Supervised));
        writers.push("openclaw");
        supervised.push("openclaw");
    }
    let approvals = Arc::new(Approvals::default());
    let setup = Setup {
        approver: Some(approvals.clone()),
        gateway,
        ..Setup::default()
    };
    hive_with(members, &model, root, setup).await?;
    check_writes(root, &model, &writers)?;
    check_nothing_escaped(root, &outside, &writers)?;
    check_approvals(&approvals.asked(), &supervised)
}

/// Each writer wrote its own file once, through the host, which answered
/// its retry from the first result and refused its escapes; the read-only
/// and denied seats wrote nothing and were told why.
fn check_writes(root: &Path, model: &HiveModel, writers: &[&str]) -> Result<()> {
    let log = std::fs::read_to_string(root.join("writes.log")).unwrap_or_default();
    let runs = |id: &str| {
        log.lines()
            .filter(|line| line.split(' ').next() == Some(id))
            .count()
    };

    for &id in writers {
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
        let results = edit_results(model, id)?;
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
        let results = edit_results(model, id)?;
        ensure!(
            results.iter().all(|result| result.contains(reason)),
            "{id} was not told every write was refused ({reason}): {results:?}"
        );
    }

    Ok(())
}

/// Nothing landed outside a workspace: no escape file beside them, and
/// nothing in `outside`, where the absolute writes aimed.
fn check_nothing_escaped(root: &Path, outside: &Path, writers: &[&str]) -> Result<()> {
    let beside: BTreeSet<String> = std::fs::read_dir(root.join("workspaces"))?
        .map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned()))
        .collect::<std::io::Result<_>>()?;
    let seats: BTreeSet<String> = writers
        .iter()
        .chain(&["reader", "refused"])
        .map(|id| (*id).to_owned())
        .collect();
    ensure!(beside == seats, "the workspaces directory holds {beside:?}");
    let outside_entries = std::fs::read_dir(outside)?.count();
    ensure!(
        outside_entries == 0,
        "{outside_entries} files landed outside"
    );

    Ok(())
}

/// The approver judged exactly the `supervised` seats and `refused`, and
/// each supervised edit once: its retry was answered from the host's
/// record without asking again.
fn check_approvals(asked: &[ToolApproval], supervised: &[&str]) -> Result<()> {
    let judged: BTreeSet<&str> = asked.iter().map(|request| request.seat.as_str()).collect();
    let expected: BTreeSet<&str> = supervised.iter().copied().chain(["refused"]).collect();
    ensure!(judged == expected, "the approver judged {judged:?}");
    for &id in supervised {
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
