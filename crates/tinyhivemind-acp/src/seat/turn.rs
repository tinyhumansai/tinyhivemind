//! One turn: a `session/prompt`, and what the agent sends until it answers.
//!
//! While the prompt runs the turn holds the connection's route. It reports
//! the agent's text and tool calls as [`Progress`], and answers each
//! `session/request_permission`: a call to the host's own MCP server is
//! allowed here, since the tool server judges hive tools itself; any other
//! goes to the seat's approver, and is refused if the approver does not
//! answer in time. Requests for a file system or a terminal, which this
//! client never offered, are refused.
use super::Live;
use crate::connection::Incoming;
use crate::error::Result;
use crate::protocol::{self, Update};
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;
use tinyhivemind_hives::TurnDisposition;
use tinyhivemind_host::harness::{
    Event, Progress, SeatPlan, ToolApproval, ToolDecision, TurnUsage,
};

/// A turn to run.
pub(super) struct Turn<'a> {
    pub(super) live: &'a Live,
    pub(super) seat: &'a str,
    pub(super) server: &'a str,
    pub(super) id: &'a str,
    pub(super) session: &'a str,
    pub(super) plan: &'a SeatPlan,
    pub(super) approval: Duration,
}

/// How a turn ended.
#[derive(Debug)]
pub(super) struct Ended {
    pub(super) reply: String,
    pub(super) usage: Option<TurnUsage>,
    pub(super) disposition: TurnDisposition,
}

/// What a tool call was, as its `tool_call` update said.
struct Call {
    title: String,
    server: Option<String>,
    raw: Value,
}

impl Turn<'_> {
    /// Prompt with `text` and follow the turn until the prompt is answered.
    pub(super) async fn run(self, text: &str) -> Result<Ended> {
        let connection = &self.live.connection;
        let mut incoming = connection.route();
        let prompt = connection.call("session/prompt", protocol::prompt(self.session, text));
        tokio::pin!(prompt);
        let mut reply = String::new();
        let mut calls = HashMap::new();
        let answered = loop {
            tokio::select! {
                answered = &mut prompt => break answered,
                Some(message) = incoming.recv() => {
                    self.handle(message, &mut reply, &mut calls).await;
                }
            }
        };
        // What the agent sent just before it answered still belongs to the turn.
        while let Ok(message) = incoming.try_recv() {
            self.handle(message, &mut reply, &mut calls).await;
        }
        connection.unroute();
        let result = answered?;
        let reason = result["stopReason"].as_str().unwrap_or("end_turn");
        let disposition =
            protocol::failure(reason).map_or(TurnDisposition::Completed, TurnDisposition::Failed);
        Ok(Ended {
            reply,
            usage: protocol::usage(&result),
            disposition,
        })
    }

    fn report(&self, progress: Progress) {
        let event = Event::Progress {
            turn_id: self.id.to_owned(),
            progress,
        };
        self.plan.observer.event(self.seat, event);
    }

    async fn handle(
        &self,
        message: Incoming,
        reply: &mut String,
        calls: &mut HashMap<String, Call>,
    ) {
        match message {
            Incoming::Notification { method, params } => {
                if method != "session/update" || params["sessionId"] != self.session {
                    return;
                }
                match protocol::update(&params["update"]) {
                    Update::Text(chunk) => {
                        reply.push_str(&chunk);
                        self.report(Progress::Text { delta: chunk });
                    }
                    Update::ToolCall { id, title, server } => {
                        self.report(Progress::ToolCall {
                            call_id: id.clone(),
                            tool: title.clone(),
                        });
                        let raw = params["update"]["rawInput"].clone();
                        calls.insert(id, Call { title, server, raw });
                    }
                    Update::ToolEnded { id, failed } => {
                        self.report(Progress::ToolResult {
                            call_id: id,
                            failed,
                        });
                    }
                    Update::Other => {}
                }
            }
            Incoming::Request { id, method, params } => {
                let answer = if method == "session/request_permission" {
                    self.permission(&params, calls).await
                } else {
                    let error = format!("{method} is not offered");
                    let _ = self.live.connection.refuse(id, &error).await;
                    return;
                };
                let _ = self.live.connection.respond(id, answer).await;
            }
        }
    }

    /// The answer to a permission request: allowed for the host's own
    /// server, otherwise the approver's.
    async fn permission(&self, params: &Value, calls: &HashMap<String, Call>) -> Value {
        let options = &params["options"];
        let tool_call = &params["toolCall"];
        let call_id = tool_call["toolCallId"].as_str().unwrap_or_default();
        let known = calls.get(call_id);
        let server = tool_call["rawInput"]["server"]
            .as_str()
            .or_else(|| known.and_then(|call| call.server.as_deref()));
        if server == Some(self.server) {
            return protocol::allow(options).map_or_else(
                || protocol::refuse(options),
                |option| protocol::selected(&option),
            );
        }
        let title = tool_call["title"]
            .as_str()
            .or_else(|| known.map(|call| call.title.as_str()))
            .unwrap_or_default();
        let arguments = Some(&tool_call["rawInput"])
            .filter(|raw| !raw.is_null())
            .or_else(|| known.map(|call| &call.raw))
            .cloned()
            .unwrap_or(Value::Null);
        let request = ToolApproval {
            seat: self.seat.to_owned(),
            turn_id: self.id.to_owned(),
            call_id: call_id.to_owned(),
            tool: title.to_owned(),
            arguments,
        };
        let decided = tokio::time::timeout(self.approval, self.plan.approver.decide(request)).await;
        match (decided, protocol::allow(options)) {
            (Ok(ToolDecision::Allow), Some(option)) => protocol::selected(&option),
            _ => protocol::refuse(options),
        }
    }
}
