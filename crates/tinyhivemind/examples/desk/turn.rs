//! Getting something out of one turn, however it ends.
//!
//! A turn is one child process, and the interesting part is not the happy
//! path: it is the ladder of recoveries below it. A stream that hangs, an
//! upstream provider that refuses, a seat that spends its whole budget inside
//! tool calls and never speaks — each of those has to end with the room
//! holding *something*, because a turn the room never heard is twenty minutes
//! nobody gets back.
//!
//! The ladder, in order: run it; retry once if the provider errored and
//! nothing came back; restart a hung stream on a fresh session; ask the seat
//! to land what it has, with its tools still attached, so the work reaches the
//! shared workspace; and only then fall back to a tool-less completion, which
//! can make a seat speak but cannot make it save.

use std::{collections::HashMap, path::Path, time::Duration};

use crate::{BoxError, agent, chat, mcp};

/// How many times one turn may be restarted after a stalled stream.
const STALL_RESTARTS: usize = 1;

/// How long a seat gets to land its work: write it down, then speak.
///
/// A seat that spends its whole working budget inside tool calls has both
/// said nothing *and* left nothing behind, so the next seat starts from an
/// empty directory. This second phase runs in the seat's own session with its
/// tools still attached, which is what a tool-less wrap-up cannot do: it can
/// summarize a turn but it cannot save one.
const LANDING_TIMEOUT: Duration = Duration::from_secs(720);

/// Everything one turn's delivery needs from the desk around it.
pub(crate) struct Delivery<'a> {
    /// The agent CLI this turn runs on.
    pub(crate) runner: &'a agent::AgentRunner,
    /// The tool-less channel of last resort.
    pub(crate) wrapup: &'a chat::Chat,
    /// Where the seat's tool calls to the room are collected.
    pub(crate) outbox: &'a Path,
    /// The name every raw event stream from this turn is filed under.
    pub(crate) label: String,
    /// Whose turn it is.
    pub(crate) seat_id: &'a str,
    /// The seat's prior CLI session, when one is being resumed.
    pub(crate) resumed: Option<&'a str>,
}

/// Run one turn and get something out of it, or nothing at all.
///
/// `Ok(None)` means the seat forfeits: every rung of the ladder was tried and
/// the room heard nothing. The caller moves on rather than appending silence.
///
/// The returned vector names the seats a `desk_dm` addressed, and is empty for
/// a message to the whole desk.
///
/// # Errors
///
/// Returns a spawn or wait failure from the agent CLI. A model that answers
/// nothing is not an error here; it is the ladder's whole reason for existing.
pub(crate) fn deliver(
    delivery: &Delivery<'_>,
    prompt: &str,
    sessions: &mut HashMap<String, String>,
    tokens: &mut u64,
) -> Result<Option<(agent::TurnOutput, Vec<String>)>, BoxError> {
    mcp::clear_outbox(delivery.outbox);
    let mut output = delivery.runner.run(
        prompt,
        &delivery.label,
        delivery.runner.timeout(),
        delivery.resumed,
    )?;
    *tokens += output.tokens;
    if output.timed_out || output.stalled {
        // The process was killed, so its session is spent. A killed
        // session cannot be resumed — the next run on it exits at once
        // having emitted nothing — so keeping the id would make every
        // later turn resume into silence. That is what turned a room
        // which had been working into one that stalled on every turn.
        sessions.remove(delivery.seat_id);
    } else if let Some(id) = output.session.clone() {
        sessions.insert(delivery.seat_id.to_string(), id);
    }
    if let Some(error) = output.error.clone() {
        // Say so. An upstream failure that reads as silence is how an hour
        // goes into diagnosing a model that was never asked.
        println!(
            "   !! agent error: {}",
            error.chars().take(180).collect::<String>()
        );
        if output.message.trim().is_empty() {
            println!("   retrying the turn once");
            output = delivery.runner.run(
                prompt,
                &format!("{}-retry", delivery.label),
                delivery.runner.timeout(),
                output.session.as_deref().or(delivery.resumed),
            )?;
            *tokens += output.tokens;
        }
    }
    // One hung stream should cost a couple of minutes, not the turn. Each
    // restart is a fresh session on the same prompt; the seat's earlier
    // work is in the workspace, so what a restart repeats is orientation
    // rather than the work itself.
    let mut restarts = 0;
    while output.stalled && restarts < STALL_RESTARTS {
        restarts += 1;
        println!(
            "   !! stalled after {:?} of silence - restart {restarts}",
            output.elapsed
        );
        output = delivery.runner.run(
            prompt,
            &format!("{}-restart{restarts}", delivery.label),
            delivery.runner.timeout(),
            None,
        )?;
        *tokens += output.tokens;
    }
    // What the seat said, it said by calling a tool. Free text is its own
    // thinking and reaches nobody; the fence remains only as a fallback for
    // an agent CLI that cannot reach the desk's tools.
    let dm_to = settle(delivery.outbox, &mut output);
    if !output.timed_out && !output.message.trim().is_empty() {
        return Ok(Some((output, dm_to)));
    }
    let Some(dm_to) = land(delivery, prompt, &mut output, sessions, tokens)? else {
        return Ok(None);
    };
    Ok(Some((output, dm_to)))
}

/// Ask a turn that said nothing to land what it has, and then to speak.
///
/// Two phases, because the failure has two halves. First the seat is asked to
/// write its work down in its own session, tools still attached, so the code
/// and the notes reach the shared workspace where the next seat can run them
/// — a tool-less wrap-up can summarize a turn but it cannot save one. Only if
/// that also runs out of time does the tool-less channel take over.
///
/// `Ok(None)` means every rung was tried and the room heard nothing.
///
/// # Errors
///
/// Returns a spawn or wait failure from the agent CLI.
fn land(
    delivery: &Delivery<'_>,
    prompt: &str,
    output: &mut agent::TurnOutput,
    sessions: &mut HashMap<String, String>,
    tokens: &mut u64,
) -> Result<Option<Vec<String>>, BoxError> {
    // Two phases, because the failure has two halves. First ask the
    // seat to land what it has: same session, tools still attached, so
    // the working code and the notes reach the shared workspace where
    // the next seat can run them. Only if that also runs out of time
    // does the tool-less channel take over, which can make a seat speak
    // but cannot make it save.
    println!(
        "   !! {} after {:?} - asking the seat to land it",
        if output.timed_out {
            "timed out"
        } else {
            "silent"
        },
        output.elapsed
    );
    let landing = format!(
        "Stop the investigation here; do not open a new line of work.\n\n\
         Two things, in order. First write down what this turn established, \
         so it outlives this process: put your notes in NOTES.md and any \
         working code in named .py files in this workspace, which every \
         seat shares. Second, call `desk_post` once, saying what you \
         established, what you did not finish, which files you wrote, and \
         the one seat you need next - that seat named first.\n\n{prompt}"
    );
    let mut landed = delivery.runner.run(
        &landing,
        &format!("{}-landing", delivery.label),
        LANDING_TIMEOUT,
        output.session.as_deref().or(delivery.resumed),
    )?;
    if !landed.posted && landed.tokens == 0 {
        // Nothing at all came back — no events, no complaint. Resuming
        // a session whose process was killed exits silently, and a
        // landing that cannot start is a landing that cannot save. Try
        // again on a fresh session: it loses the seat's working
        // context, which is the lesser of the two losses.
        println!("   landing on the resumed session produced nothing; retrying fresh");
        landed = delivery.runner.run(
            &landing,
            &format!("{}-landing-fresh", delivery.label),
            LANDING_TIMEOUT,
            None,
        )?;
    }
    *tokens += landed.tokens;
    if let Some(id) = landed.session.clone() {
        sessions.insert(delivery.seat_id.to_string(), id);
    }
    let dm_to = settle(delivery.outbox, &mut landed);
    let salvage = if !landed.posted || landed.message.trim().is_empty() {
        let wrap = format!(
            "{prompt}\n\n## What you actually ran this turn\n{}\n\nYou have no \
             tools. Your working turn ended before you posted anything. Write the \
             message now, in one block wrapped in <<<POST and POST>>>: what you \
             established, what you did not finish, and the one seat you need next. \
             Ground every claim in the commands above; if they do not establish \
             something, say it is not established.",
            agent::truncate_work_log(&output.work_log)
        );
        let text = delivery.wrapup.complete(&wrap);
        if !text.trim().is_empty() {
            println!("   wrap-up posted through the router with no tools attached");
        }
        agent::extract_post(&text)
    } else {
        println!("   landed in the seat's own session, files included");
        landed.message
    };
    if salvage.trim().is_empty() {
        println!("   !! nothing salvaged; the seat forfeits this turn");
        return Ok(None);
    }
    output.message = salvage;
    Ok(Some(dm_to))
}

/// Take what a turn said through the room's tools, over what it narrated.
///
/// Returns the seats a private message named, empty for a message to the desk.
///
/// A seat may call `desk_post` more than once — it is told not to, and it will
/// anyway. The last call stands: a seat that posts a partial result and then a
/// settled one meant the second, and one message per turn is the rule the whole
/// design rests on.
fn settle(outbox: &Path, output: &mut agent::TurnOutput) -> Vec<String> {
    let said = mcp::drain_outbox(outbox);
    let Some(utterance) = said.last() else {
        return Vec::new();
    };
    if said.len() > 1 {
        println!("   {} messages this turn; the last one stands", said.len());
    }
    output.message = utterance.message().to_string();
    output.posted = true;
    match utterance {
        mcp::Utterance::Post { .. } => Vec::new(),
        mcp::Utterance::Dm { to, .. } => to.clone(),
    }
}
