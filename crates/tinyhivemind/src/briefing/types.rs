//! Stable team briefing records.

use crate::{SESSION_WINDOW, SessionMessage, ThreadLine, pins::Pin};
use serde::{Deserialize, Serialize};

/// One teammate described to the initialized viewer.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct BriefedTeammate {
    /// Stable agent id and mention handle without the leading `@`.
    pub id: String,
    /// Human-readable agent label.
    pub label: String,
    /// Optional host-supplied team role.
    pub role: Option<String>,
    /// Optional host-supplied agent description.
    pub description: Option<String>,
}

/// How much of the window one message may spend.
///
/// The transcript a turn reads is bounded — [`window`](Self::window) messages
/// — so every message is spending a scarce, shared resource. An agent that
/// writes six paragraphs pushes five earlier messages out of everybody else's
/// view, including the one that answered the question. The policy is stated in
/// the briefing so the agent knows the budget it is writing against, and it is
/// a *budget*, not a truncation: this crate never edits what somebody wrote.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct BrevityPolicy {
    /// Soft per-message character budget.
    pub message_chars: usize,
    /// Messages a turn is shown, which is what the budget is spent out of.
    pub window: usize,
}

impl BrevityPolicy {
    /// The default budget: one screen of text, against the default window.
    pub const DEFAULT: Self = Self {
        message_chars: 600,
        window: SESSION_WINDOW,
    };

    /// Characters by which a message overruns the budget, when it does.
    ///
    /// Reported, never enforced. A host may nudge, may ask for a shorter
    /// message, or may do nothing — but nothing here rewrites an authored
    /// message, because a transcript that disagrees with what was said is
    /// worse than a long one.
    #[must_use]
    pub fn overrun(&self, content: &str) -> Option<usize> {
        content
            .chars()
            .count()
            .checked_sub(self.message_chars)
            .filter(|over| *over > 0)
    }

    /// Render the budget as one briefing rule line.
    #[must_use]
    pub fn rule_text(&self) -> String {
        format!(
            "- This conversation shows about {} messages; keep a message under {} characters, one point each, and pin or search rather than restating.",
            self.window, self.message_chars
        )
    }
}

impl Default for BrevityPolicy {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Ephemeral context describing one viewer's team and shared desk.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct TeamBriefing {
    /// Agent id receiving the briefing.
    pub viewer_id: String,
    /// Canonical desk id.
    pub desk_id: String,
    /// Operator-facing desk name.
    pub desk_name: String,
    /// Other active teammates in deterministic effective order.
    pub teammates: Vec<BriefedTeammate>,
    /// How much of the bounded window one message may spend.
    #[serde(default)]
    pub brevity: BrevityPolicy,
}

/// One host-supplied block of context that is not in the log and not a thread.
///
/// Board state, open work, attachments — anything a host wants a turn to know
/// and this crate cannot compute. It is carried beside the operator's message,
/// never appended to it, so nothing downstream has to cut it back off before
/// reasoning about what was asked.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct BriefingNote {
    /// Short heading, e.g. `Work raised in this conversation`.
    pub heading: String,
    /// Lines rendered under that heading, in the host's order.
    pub lines: Vec<String>,
}

/// Context for a turn that is neither the team nor the transcript.
///
/// [`threads`](Self::threads) is a fold this crate computes;
/// [`notes`](Self::notes) is whatever the host adds. Both stay separate from
/// [`SessionInitialization::history`], which is the log and only the log.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionContext {
    /// Live threads in this desk, newest activity first.
    pub threads: Vec<ThreadLine>,
    /// Messages pinned in this conversation, most recently pinned first.
    #[serde(default)]
    pub pins: Vec<Pin>,
    /// Host-supplied blocks this crate cannot compute.
    pub notes: Vec<BriefingNote>,
}

/// Separate ephemeral briefing and durable attributed history for a new turn.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionInitialization {
    /// Ephemeral team context; it is not part of the host log.
    pub briefing: TeamBriefing,
    /// Threads and host notes; typed, and never merged into a message.
    pub context: SessionContext,
    /// Chronological attributed messages from the host log.
    pub history: Vec<SessionMessage>,
}
