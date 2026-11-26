//! One turn: `channel_web_chat` on the seat's thread, followed on `/events`.
//!
//! The order matters, and so does what is ignored:
//!
//! 1. Subscribe to `/events` for the turn's own `client_id` first, since
//!    events go only to whoever is subscribed when they happen, and the
//!    first ones can be out before `channel_web_chat` answers.
//! 2. Submit, and keep the `request_id` it returns in the seat's [`Active`],
//!    which is what a cancellation names. The turn is never submitted again.
//! 3. Read events of that `request_id` only. `approval_request` is the
//!    exception: its `request_id` is the approval's, and the seat's approver
//!    decides it through `approval_decide`.
//! 4. `chat_done` ends the turn with its reply and usage, unless the turn
//!    reached [`ITERATION_CAP`] `iteration_start` events, which the binary
//!    reports as a normal end. `chat_error` and `chat_cancelled` fail it.
//!    A seat limit below the cap fails the turn at the first iteration past
//!    it, and the seat runner then cancels it.
//!
//! [`cancel`] is `channel_web_cancel` scoped to the request, then a wait for
//! that request's end on a fresh subscription.
use crate::capability::ITERATION_CAP;
use crate::rpc::{self, Client, Events};
use serde_json::{Value, json};
use std::sync::{Mutex, PoisonError};
use tinyhivemind_host::harness::{
    Approver, Event, Observer, Progress, ToolApproval, ToolDecision, TurnUsage,
};
use tokio::sync::watch;

/// Result of a turn.
pub(crate) type Result<T> = std::result::Result<T, Error>;

/// Why a turn has no outcome.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    /// The API failed.
    #[error(transparent)]
    Api(#[from] rpc::Error),
    /// `channel_web_chat` answered without accepting the message.
    #[error("the instance did not accept the turn: {0}")]
    NotAccepted(String),
    /// The event stream ended before the turn did: the instance went away.
    #[error("the event stream ended before the turn did")]
    StreamEnded,
    /// The seat is being stopped.
    #[error("the turn was closed by a stop")]
    Closed,
}

/// The submitted turn of a seat, which a cancellation names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Active {
    /// The seat's thread.
    pub(crate) thread: String,
    /// The turn's own `client_id`.
    pub(crate) client_id: String,
    /// What `channel_web_chat` returned, once it has.
    pub(crate) request_id: Option<String>,
}

/// How a turn ended.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Ended {
    /// The model's reply.
    pub(crate) reply: Option<String>,
    /// What the turn used, if it said.
    pub(crate) usage: Option<TurnUsage>,
    /// Why it failed, if it did.
    pub(crate) failure: Option<String>,
}

/// What one turn runs with.
pub(crate) struct Turn<'a> {
    pub(crate) client: &'a Client,
    pub(crate) seat: &'a str,
    pub(crate) thread: &'a str,
    /// The coordinator's turn id.
    pub(crate) id: &'a str,
    pub(crate) client_id: String,
    /// The seat's iteration limit, at most [`ITERATION_CAP`].
    pub(crate) iterations: usize,
    pub(crate) observer: &'a dyn Observer,
    pub(crate) approver: &'a dyn Approver,
    /// Becomes true when the seat is stopped.
    pub(crate) closed: watch::Receiver<bool>,
}

fn set(active: &Mutex<Option<Active>>, value: Option<Active>) {
    *active.lock().unwrap_or_else(PoisonError::into_inner) = value;
}

/// Run `turn` with `message`. `active` holds the submitted turn until it has
/// ended, so that a turn dropped meanwhile can be cancelled.
pub(crate) async fn run(
    mut turn: Turn<'_>,
    message: &str,
    active: &Mutex<Option<Active>>,
) -> Result<Ended> {
    let mut events = turn.client.events(&turn.client_id).await?;
    let mut submitted = Active {
        thread: turn.thread.to_owned(),
        client_id: turn.client_id.clone(),
        request_id: None,
    };
    set(active, Some(submitted.clone()));
    let params = json!({"client_id": turn.client_id, "thread_id": turn.thread, "message": message});
    let ack = turn
        .client
        .call("openhuman.channel_web_chat", params)
        .await?;
    let ack = &ack["result"];
    let request_id = ack["request_id"]
        .as_str()
        .filter(|_| ack["accepted"] == true);
    let request_id = request_id.ok_or_else(|| Error::NotAccepted(ack.to_string()))?;
    submitted.request_id = Some(request_id.to_owned());
    set(active, Some(submitted));
    let mut closed = turn.closed.clone();
    let ended = tokio::select! {
        ended = follow(&turn, request_id, &mut events) => ended,
        _ = closed.wait_for(|closed| *closed) => Err(Error::Closed),
    };
    turn.closed = closed;
    let (ended, over) = ended?;
    if over {
        set(active, None);
    }
    Ok(ended)
}

/// Read events until the request ends, or fails while it runs on: whether
/// the instance's turn is over.
async fn follow(turn: &Turn<'_>, request_id: &str, events: &mut Events) -> Result<(Ended, bool)> {
    let mut iterations = 0_usize;
    let progress = |progress| Event::Progress {
        turn_id: turn.id.to_owned(),
        progress,
    };
    while let Some(event) = events.next().await? {
        let name = event["event"].as_str().unwrap_or_default();
        if name == "approval_request" {
            approve(turn, &event).await?;
            continue;
        }
        if event["request_id"] != request_id {
            continue;
        }
        let text = |key: &str| event[key].as_str().unwrap_or_default().to_owned();
        match name {
            "iteration_start" => {
                iterations += 1;
                let iteration = u32::try_from(iterations).unwrap_or(u32::MAX);
                let step = progress(Progress::ModelCall { iteration });
                turn.observer.event(turn.seat, step);
                if turn.iterations < ITERATION_CAP && iterations > turn.iterations {
                    let failure = format!(
                        "the turn started iteration {iterations}, past the seat's limit of {}",
                        turn.iterations
                    );
                    let ended = Ended {
                        failure: Some(failure),
                        ..Ended::default()
                    };
                    return Ok((ended, false));
                }
            }
            "text_delta" if event["delta_kind"] == "text" => {
                let delta = Progress::Text {
                    delta: text("delta"),
                };
                turn.observer.event(turn.seat, progress(delta));
            }
            "tool_call" => {
                let call = Progress::ToolCall {
                    call_id: text("tool_call_id"),
                    tool: text("tool_name"),
                };
                turn.observer.event(turn.seat, progress(call));
            }
            "tool_result" => {
                let result = Progress::ToolResult {
                    call_id: text("tool_call_id"),
                    failed: event["success"] != true,
                };
                turn.observer.event(turn.seat, progress(result));
            }
            "chat_done" => {
                let failure = (iterations >= ITERATION_CAP).then(|| {
                    format!(
                        "the turn reached the binary's cap of {ITERATION_CAP} iterations, which it reports as a normal end"
                    )
                });
                let ended = Ended {
                    reply: event["full_response"].as_str().map(str::to_owned),
                    usage: usage(&event["usage"]),
                    failure,
                };
                return Ok((ended, true));
            }
            "chat_error" | "chat_cancelled" => {
                let failure = format!("{name} ({}): {}", text("error_type"), text("message"));
                let ended = Ended {
                    failure: Some(failure),
                    ..Ended::default()
                };
                return Ok((ended, true));
            }
            _ => {}
        }
    }
    Err(Error::StreamEnded)
}

fn usage(usage: &Value) -> Option<TurnUsage> {
    let count = |key: &str| usage[key].as_u64();
    Some(TurnUsage {
        input_tokens: count("input_tokens")?,
        cached_input_tokens: count("cached_input_tokens").unwrap_or(0),
        output_tokens: count("output_tokens").unwrap_or(0),
        reasoning_tokens: 0,
    })
}

/// Ask the seat's approver, and tell the instance.
async fn approve(turn: &Turn<'_>, event: &Value) -> Result<()> {
    let id = event["request_id"].as_str().unwrap_or_default().to_owned();
    let request = ToolApproval {
        seat: turn.seat.to_owned(),
        turn_id: turn.id.to_owned(),
        call_id: id.clone(),
        tool: event["tool_name"].as_str().unwrap_or_default().to_owned(),
        arguments: event["args"].clone(),
    };
    let decision = match turn.approver.decide(request).await {
        ToolDecision::Allow => "approve_once",
        ToolDecision::Deny { .. } => "deny",
    };
    let params = json!({"request_id": id, "decision": decision});
    turn.client
        .call("openhuman.approval_decide", params)
        .await?;
    Ok(())
}

/// Cancel `active`, returning once its request has ended.
pub(crate) async fn cancel(client: &Client, active: &Active) -> Result<()> {
    let mut events = client.events(&active.client_id).await?;
    let mut params = json!({"client_id": active.client_id, "thread_id": active.thread});
    if let Some(request_id) = &active.request_id {
        params["request_id"] = json!(request_id);
    }
    let ack = client.call("openhuman.channel_web_cancel", params).await?;
    let Some(cancelled) = ack["result"]["request_id"].as_str() else {
        // Nothing was running, so no end follows.
        return Ok(());
    };
    while let Some(event) = events.next().await? {
        let ends = matches!(
            event["event"].as_str(),
            Some("chat_error" | "chat_cancelled" | "chat_done")
        );
        if ends && event["request_id"] == cancelled {
            return Ok(());
        }
    }
    Err(Error::StreamEnded)
}

#[cfg(test)]
mod test;
