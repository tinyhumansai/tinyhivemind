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

use tinyhivemind::speech::{Utterance, fence};

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

/// What a turn asked the room to do, once its tool calls are drained.
pub(crate) struct Said {
    /// The utterance the room commits, as the library names it.
    ///
    /// A turn that reached the tools drained one; a turn that fell back to the
    /// fence, or that was wrapped up by a tool-less completion, spoke to the
    /// whole desk and is carried as a [`Utterance::Post`] so that every path
    /// out of a turn commits through the same fold.
    pub(crate) utterance: Utterance,
}

/// Run one turn and get something out of it, or nothing at all.
///
/// `Ok(None)` means the seat forfeits: every rung of the ladder was tried and
/// the room heard nothing. The caller moves on rather than appending silence.
///
/// The returned [`Said`] names the seats a `desk_dm` addressed and whether the
/// seat reported the work finished.
///
/// # Errors
///
/// Returns a spawn or wait failure from the agent CLI. A model that answers
/// nothing is not an error here; it is the ladder's whole reason for existing.
pub(crate) async fn deliver(
    delivery: &Delivery<'_>,
    prompt: &str,
    sessions: &mut HashMap<String, String>,
    tokens: &mut u64,
) -> Result<Option<(agent::TurnOutput, Said)>, BoxError> {
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
    //
    // The gate is whether the seat *spoke*, not whether it produced text. A
    // turn whose provider fails mid-flight still leaves its narration in
    // `output.message`, and a host that reads that as speech appends
    // "Let me verify the small cases" to the transcript as though it were a
    // message — which is what run 29 did on both of its turns, twice
    // stranding twenty minutes of real work behind a sentence that addressed
    // nobody.
    if !output.timed_out
        && let Some(said) = settle(delivery.outbox, &mut output)
    {
        return Ok(Some((output, said)));
    }
    let Some(said) = land(delivery, prompt, &mut output, sessions, tokens).await? else {
        return Ok(None);
    };
    Ok(Some((output, said)))
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
async fn land(
    delivery: &Delivery<'_>,
    prompt: &str,
    output: &mut agent::TurnOutput,
    sessions: &mut HashMap<String, String>,
    tokens: &mut u64,
) -> Result<Option<Said>, BoxError> {
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
    // The landing's whole purpose is to get the work onto disk, so what it
    // wrote is exactly what the room most needs told about. Without this the
    // feedthrough row reports the *working* turn's files and silently omits
    // the ones the rescue produced — which is every file, on a turn that ran
    // out of budget before it wrote anything.
    absorb_written(output, &landed);
    if let Some(said) = settle(delivery.outbox, &mut landed) {
        println!("   landed in the seat's own session, files included");
        output.message = said.utterance.message().to_string();
        return Ok(Some(said));
    }
    let salvage = {
        let wrap = format!(
            "{prompt}\n\n## What you actually ran this turn\n{}\n\nYou have no \
             tools. Your working turn ended before you posted anything. Write the \
             message now, in one block wrapped in <<<POST and POST>>>: what you \
             established, what you did not finish, and the one seat you need next. \
             Ground every claim in the commands above; if they do not establish \
             something, say it is not established.",
            agent::truncate_work_log(&output.work_log)
        );
        // One retry, and only for a failure that says trying again could
        // work. A refusal answered twice is a refusal; a rate limit answered
        // once is a turn thrown away for a reason the provider named.
        let mut answer = delivery.wrapup.complete(&wrap).await;
        if answer.worth_retrying() {
            println!("   wrap-up is worth one more attempt; retrying");
            answer = delivery.wrapup.complete(&wrap).await;
        }
        match &answer {
            chat::Outcome::Answered(_) => {
                println!("   wrap-up posted through the router with no tools attached");
            }
            other => println!("   !! the wrap-up channel produced nothing: {other:?}"),
        }
        fence::extract_post(answer.text())
    };
    if salvage.trim().is_empty() {
        println!("   !! nothing salvaged; the seat forfeits this turn");
        return Ok(None);
    }
    output.message.clone_from(&salvage);
    // The tool-less rung has no tool to call, so what it produced is a post
    // to the whole desk or it is nothing.
    Ok(Some(Said {
        utterance: Utterance::Post { message: salvage },
    }))
}

/// Fold a rescue turn's written paths into the turn the room is told about.
///
/// Order is the order they were written, the working turn's first, and a path
/// written in both phases is named once.
fn absorb_written(output: &mut agent::TurnOutput, landed: &agent::TurnOutput) {
    for path in &landed.files_written {
        if !output.files_written.contains(path) {
            output.files_written.push(path.clone());
        }
    }
}

/// What a turn said to the room, or `None` when it said nothing.
///
/// Three outcomes, and the third is the one worth naming. A seat that called a
/// room tool spoke; a seat that wrote a fence spoke, because the fence is the
/// documented fallback for an agent CLI that cannot reach the tools. A seat
/// that did neither *narrated*, and narration reaches nobody — that is what
/// [`agent::TurnOutput::posted`] has always meant, and returning it as a
/// message is how twenty minutes of real work reached the room as
/// "Let me verify the small cases and understand the structure better."
///
/// A seat may call `desk_post` more than once — it is told not to, and it will
/// anyway. The last call stands: a seat that posts a partial result and then a
/// settled one meant the second, and one message per turn is the rule the whole
/// design rests on.
fn settle(outbox: &Path, output: &mut agent::TurnOutput) -> Option<Said> {
    let spoken = mcp::drain_outbox(outbox);
    if let Some(utterance) = spoken.last() {
        if spoken.len() > 1 {
            println!(
                "   {} messages this turn; the last one stands",
                spoken.len()
            );
        }
        output.message = utterance.message().to_string();
        output.posted = true;
        return Some(Said {
            utterance: utterance.clone(),
        });
    }
    // No tool call. The fence is the documented fallback for an agent CLI
    // that cannot reach the desk's tools, and `posted` is exactly whether one
    // was written — so a fenced message still counts as speech.
    if output.posted && !output.message.trim().is_empty() {
        println!("   spoke through the fence rather than the tools");
        return Some(Said {
            utterance: Utterance::Post {
                message: output.message.clone(),
            },
        });
    }
    // Everything else is thinking. It reaches nobody, and saying so here is
    // what stops it reaching the transcript instead.
    None
}

#[cfg(test)]
mod test;
