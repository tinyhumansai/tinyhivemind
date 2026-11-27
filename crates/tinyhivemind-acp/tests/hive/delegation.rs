//! `live_*`: lead-only episodes across harnesses. Only the lead starts;
//! the seats on the other two harnesses take part because the lead asks
//! them, and a member nobody asks never runs.

use super::{claude, codex, openhuman, settle, start_hive, stop_hive};
use crate::model::{Api, Delegation, HiveModel, Seen, answered};
use anyhow::{Result, ensure};
use std::collections::BTreeSet;
use tinyhivemind_hives::{CoordinatorOptions, Message};

/// The harnesses a delegation spans, by seat id, and the model API each
/// speaks.
const HARNESSES: [(&str, Api); 3] = [
    ("openhuman", Api::Chat),
    ("claude", Api::Messages),
    ("codex", Api::Responses),
];

/// A lead-only episode across harnesses. Only `lead` starts it. The lead
/// asks the other two harnesses' seats in one conversation and ends its
/// turn; each seat asked answers there, on its own harness; the lead is
/// shown both answers on a later turn and completes; and the episode
/// settles. `idle`, a fourth member nobody asked, takes no turn. The round
/// is two wide, so the two seats asked run at once: at the default width
/// of one, an ask naming two seats fails the episode.
async fn lead_delegates(lead: &str) -> Result<()> {
    let root = tempfile::tempdir()?;
    let root = root.path();
    let asked: Vec<String> = HARNESSES
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
    let (Some(openhuman), Some(claude), Some(codex), Some(idle)) = (
        openhuman(model.port),
        claude("claude", model.port),
        codex("codex", model.port, root)?,
        claude("idle", model.port),
    ) else {
        eprintln!(
            "skipped: set THM_OPENHUMAN_CORE, THM_NODE, THM_ACP_CLAUDE, THM_ACP_CODEX and THM_CODEX"
        );
        return Ok(());
    };
    let members = vec![openhuman, claude, codex, idle];
    let options = CoordinatorOptions {
        round_width: 2,
        ..CoordinatorOptions::default()
    };
    let (coordinator, server) = start_hive(&members, root, options, None).await?;
    settle(&coordinator, vec![lead.into()]).await?;
    let transcript = coordinator.read_transcript(None)?;
    stop_hive(&members, server).await?;
    let seen = model.seen();

    // The three harnesses' seats ran, each through its own API, and the
    // member nobody asked did not.
    let ran: BTreeSet<&str> = seen.iter().map(|request| request.agent.as_str()).collect();
    let harnesses: BTreeSet<&str> = HARNESSES.iter().map(|(id, _)| *id).collect();
    ensure!(ran == harnesses, "the seats that ran: {ran:?}");
    for (id, api) in HARNESSES {
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
    for id in &asked {
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
    // that completed it and so settled the episode, showed it both. (A lead
    // that completes in the turn it asks does not end the episode: the
    // seats asked still answer, and it completes again once shown both.)
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
        "{lead} did not complete on a later turn showing both answers: {turns:#?}"
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn live_a_claude_code_lead_delegates_to_openhuman_and_codex() -> Result<()> {
    lead_delegates("claude").await
}

#[tokio::test(flavor = "multi_thread")]
async fn live_a_codex_lead_delegates_to_openhuman_and_claude_code() -> Result<()> {
    lead_delegates("codex").await
}

#[tokio::test(flavor = "multi_thread")]
async fn live_an_openhuman_lead_delegates_to_claude_code_and_codex() -> Result<()> {
    lead_delegates("openhuman").await
}
