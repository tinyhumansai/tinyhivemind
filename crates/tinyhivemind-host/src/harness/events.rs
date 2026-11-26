//! What a seat reports while it runs, and what it may have to ask.
use super::Started;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

/// Tokens used, by one turn or in total.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct TurnUsage {
    /// Input tokens, the cached ones included.
    pub input_tokens: u64,
    /// Input tokens read from a cache.
    pub cached_input_tokens: u64,
    /// Output tokens, the reasoning ones included.
    pub output_tokens: u64,
    /// Output tokens spent on reasoning.
    pub reasoning_tokens: u64,
}

impl TurnUsage {
    /// What was used between the total `earlier` and this one, or `None` if
    /// any count went down, which means they are not totals of one session.
    #[must_use]
    pub fn since(self, earlier: Self) -> Option<Self> {
        Some(Self {
            input_tokens: self.input_tokens.checked_sub(earlier.input_tokens)?,
            cached_input_tokens: self
                .cached_input_tokens
                .checked_sub(earlier.cached_input_tokens)?,
            output_tokens: self.output_tokens.checked_sub(earlier.output_tokens)?,
            reasoning_tokens: self
                .reasoning_tokens
                .checked_sub(earlier.reasoning_tokens)?,
        })
    }
}

/// One step of a running turn.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Progress {
    /// The model is being called.
    ModelCall {
        /// Its number within the turn, from one.
        iteration: u32,
    },
    /// The model wrote some of its reply.
    Text {
        /// The new text.
        delta: String,
    },
    /// The model called a tool.
    ToolCall {
        /// The harness's id for the call.
        call_id: String,
        /// The tool, by the name the model sees.
        tool: String,
    },
    /// A tool call ended.
    ToolResult {
        /// The harness's id for the call.
        call_id: String,
        /// Whether it failed.
        failed: bool,
    },
}

/// Something a seat reports.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// An instance was started for the seat and verified. Reported again
    /// whenever a runner has to start another.
    Started(Started),
    /// A turn got further.
    Progress {
        /// The turn.
        turn_id: String,
        /// The step.
        progress: Progress,
    },
    /// What one turn used, exactly once for a turn that completes. Never a
    /// running total.
    Usage {
        /// The turn.
        turn_id: String,
        /// Its usage.
        usage: TurnUsage,
    },
}

/// Receives a seat's events. Called in line, so it must not block.
pub trait Observer: Send + Sync {
    /// `seat_id` reports `event`.
    fn event(&self, seat_id: &str, event: Event);
}

/// Drops every event.
#[derive(Clone, Copy, Debug, Default)]
pub struct Ignore;

impl Observer for Ignore {
    fn event(&self, _: &str, _: Event) {}
}

/// A request to run one tool call: a harness asking about one of its own
/// tools, or the host's tool server asking about an effectful executable
/// tool of a `supervised` seat. Hive tools never ask: send and management
/// policy decide them.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToolApproval {
    /// The seat asking.
    pub seat: String,
    /// The running turn.
    pub turn_id: String,
    /// The harness's id for the call, or the model's `call` when the tool
    /// server asks.
    pub call_id: String,
    /// The tool.
    pub tool: String,
    /// Its arguments.
    pub arguments: Value,
}

/// The answer to a [`ToolApproval`].
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum ToolDecision {
    /// Run it, this once.
    Allow,
    /// Refuse it.
    Deny {
        /// Told to the model.
        reason: String,
    },
}

/// Future returned by [`Approver::decide`].
pub type ApprovalFuture = Pin<Box<dyn Future<Output = ToolDecision> + Send>>;

/// Decides whether a tool call may run, for a harness or the tool server.
pub trait Approver: Send + Sync {
    /// Decide `request`. The turn waits, within its own timeout.
    fn decide(&self, request: ToolApproval) -> ApprovalFuture;
}

/// Denies everything: the default, as every harness's built-in tools are off.
#[derive(Clone, Copy, Debug, Default)]
pub struct DenyAll;

impl Approver for DenyAll {
    fn decide(&self, _: ToolApproval) -> ApprovalFuture {
        Box::pin(async {
            ToolDecision::Deny {
                reason: "this host approves no harness tool".into(),
            }
        })
    }
}
