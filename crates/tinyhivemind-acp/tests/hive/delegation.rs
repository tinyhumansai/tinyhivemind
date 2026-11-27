//! `live_*`: lead-only episodes across harnesses. Only the lead starts;
//! the seats on the other harnesses take part because the lead asks them,
//! and a member nobody asks never runs.

use super::{
    Member, Setup, claude, codex, gateway, openclaw, openhuman, settle, start_hive, stop_hive,
};
use crate::model::{Api, Delegation, HiveModel, Seen, answered};
use anyhow::{Result, ensure};
use std::collections::BTreeSet;
use std::path::Path;
use tinyhivemind_acp::openclaw::GatewayPlan;
use tinyhivemind_hives::{CoordinatorOptions, Message};

/// The three harnesses every lead-only episode spans, by seat id, and the
/// model API each speaks.
const HARNESSES: [(&str, Api); 3] = [
    ("openhuman", Api::Chat),
    ("claude", Api::Messages),
    ("codex", Api::Responses),
];

/// `OpenClaw`, which the episodes that name it span too.
const OPENCLAW: (&str, Api) = ("openclaw", Api::Chat);

/// A lead-only episode across `harnesses`. Only `lead` starts it. The lead
/// asks every other harness's seat in one conversation and ends its turn;
/// each seat asked answers there, on its own harness; the lead is shown
/// every answer on a later turn and completes; and the episode settles.
/// `idle`, a member nobody asked, takes no turn. The round is as wide as the
/// seats asked, so they run at once: at the default width of one, an ask
/// naming two seats fails the episode.
async fn lead_delegates(lead: &str, harnesses: &[(&'static str, Api)]) -> Result<()> {
    let root = tempfile::tempdir()?;
    let root = root.path();
    let asked: Vec<String> = harnesses
        .iter()
        .map(|(id, _)| *id)
        .filter(|id| *id != lead)
        .map(str::to_owned)
        .collect();
    let delegation = Delegation {
        lead: lead.into(),
        asked: asked.clone(),
    };
    let model = HiveModel::start_delegating(delegation).await?;
    let gateway = gateway(model.port, root)?;
    let Some(members) = members(harnesses, model.port, root, gateway.as_ref())? else {
        eprintln!(
            "skipped: set THM_OPENHUMAN_CORE, THM_NODE, THM_ACP_CLAUDE, THM_ACP_CODEX, THM_CODEX and, for OpenClaw, THM_OPENCLAW"
        );
        return Ok(());
    };
    let setup = Setup {
        options: CoordinatorOptions {
            round_width: asked.len(),
            ..CoordinatorOptions::default()
        },
        gateway: gateway.filter(|_| members.iter().any(|member| member.gateway)),
        ..Setup::default()
    };
    let running = start_hive(&members, root, setup).await?;
    settle(&running.coordinator, vec![lead.into()]).await?;
    let transcript = running.coordinator.read_transcript(None)?;
    stop_hive(&members, running).await?;
    check(lead, &asked, harnesses, &transcript, &model.seen())
}

/// A seat on each of `harnesses`, then `idle`, a Claude Code seat nobody
/// asks; `None` if an agent is missing.
fn members(
    harnesses: &[(&'static str, Api)],
    port: u16,
    root: &Path,
    gateway: Option<&GatewayPlan>,
) -> Result<Option<Vec<Member>>> {
    let mut members = Vec::new();
    for (id, _) in harnesses {
        let member = match *id {
            "openhuman" => openhuman(port),
            "claude" => claude("claude", port),
            "codex" => codex("codex", port, root)?,
            _ => gateway.map(|gateway| openclaw("openclaw", port, gateway)),
        };
        let Some(member) = member else {
            return Ok(None);
        };
        members.push(member);
    }
    let Some(idle) = claude("idle", port) else {
        return Ok(None);
    };
    members.push(idle);
    Ok(Some(members))
}

/// What a lead-only episode must show: who ran and how, the one ask, the
/// answers inside its conversation, and the lead completing once shown them.
fn check(
    lead: &str,
    asked: &[String],
    harnesses: &[(&'static str, Api)],
    transcript: &[Message],
    seen: &[Seen],
) -> Result<()> {
    // Every harness's seat ran, each through its own API, and the member
    // nobody asked did not.
    let ran: BTreeSet<&str> = seen.iter().map(|request| request.agent.as_str()).collect();
    let spanned: BTreeSet<&str> = harnesses.iter().map(|(id, _)| *id).collect();
    ensure!(ran == spanned, "the seats that ran: {ran:?}");
    for &(id, api) in harnesses {
        let wrong: Vec<_> = seen
            .iter()
            .filter(|request| request.agent == id && request.api != api)
            .collect();
        ensure!(wrong.is_empty(), "{id} did not speak {api:?}: {wrong:#?}");
    }

    // The lead asked once, and the ask's row is the conversation's root.
    let asks: Vec<&Message> = transcript
        .iter()
        .filter(|message| message.sender == lead && message.body == "question")
        .collect();
    let [ask] = asks.as_slice() else {
        anyhow::bail!("{lead} asked {} times: {transcript:#?}", asks.len());
    };
    let conversation = Some(ask.sequence);

    // Each seat asked ran only in that conversation and answered there once.
    for id in asked {
        let outside: Vec<_> = seen
            .iter()
            .filter(|request| request.agent == *id && request.thread != conversation)
            .collect();
        ensure!(
            outside.is_empty(),
            "{id} ran outside the conversation: {outside:#?}"
        );
        let answers = transcript
            .iter()
            .filter(|message| {
                message.sender == *id
                    && message.body == answered(id)
                    && message.thread == conversation
            })
            .count();
        ensure!(
            answers == 1,
            "{id} answered {answers} times in the conversation: {transcript:#?}"
        );
    }

    // The lead asked before any answer existed, and its last turn, the one
    // that completed it and so settled the episode, showed it every answer. (A lead
    // that completes in the turn it asks does not end the episode: the
    // seats asked still answer, and it completes again once shown them all.)
    let turns: Vec<&Seen> = seen
        .iter()
        .filter(|request| request.agent == lead)
        .collect();
    let (Some(first), Some(last)) = (turns.first(), turns.last()) else {
        anyhow::bail!("the model never saw a turn of {lead}");
    };
    let shows = |request: &Seen, id: &String| request.context.contains(&answered(id));
    ensure!(
        !asked.iter().any(|id| shows(first, id)),
        "{lead}'s first turn already showed an answer: {}",
        first.context
    );
    ensure!(
        last.turn != first.turn && asked.iter().all(|id| shows(last, id)),
        "{lead} did not complete on a later turn showing every answer: {turns:#?}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn live_a_claude_code_lead_delegates_to_openhuman_and_codex() -> Result<()> {
    lead_delegates("claude", &HARNESSES).await
}

#[tokio::test(flavor = "multi_thread")]
async fn live_a_codex_lead_delegates_to_openhuman_and_claude_code() -> Result<()> {
    lead_delegates("codex", &HARNESSES).await
}

#[tokio::test(flavor = "multi_thread")]
async fn live_an_openhuman_lead_delegates_to_claude_code_and_codex() -> Result<()> {
    lead_delegates("openhuman", &HARNESSES).await
}

/// Every harness's seat, `OpenClaw`'s included.
fn all_four() -> Vec<(&'static str, Api)> {
    HARNESSES.into_iter().chain([OPENCLAW]).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn live_an_openclaw_lead_delegates_to_every_other_harness() -> Result<()> {
    lead_delegates("openclaw", &all_four()).await
}

#[tokio::test(flavor = "multi_thread")]
async fn live_a_codex_lead_delegates_to_openclaw_and_every_other_harness() -> Result<()> {
    lead_delegates("codex", &all_four()).await
}
