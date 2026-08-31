//! Stable caller-owned continuous-sharing values.

use crate::{Conversation, Sequence, SessionMessage};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Host-owned progress for one initialized agent conversation.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SharingState {
    /// Conversation whose accepted transcript this state describes.
    pub conversation: Conversation,
    /// Exclusive lower boundary accepted by the agent session.
    pub watermark: Sequence,
    /// Later rows already accepted through a concurrent path.
    pub present_above_watermark: BTreeSet<Sequence>,
}

/// Borrowed inputs for one stateless sharing walk.
///
/// This is a call-only view and intentionally has no serde representation.
#[derive(Clone, Copy, Debug)]
pub struct SharingQuery<'a> {
    /// Conversation the next turn wants to enter.
    pub desired_conversation: &'a Conversation,
    /// Conversation to which the host session is currently bound.
    pub current_conversation: &'a Conversation,
    /// Last caller-committed sharing progress.
    pub state: &'a SharingState,
    /// Exclusive sequence of the next triggering message.
    pub before: Sequence,
}

/// Chronological additions and the state a host may commit after acceptance.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SessionDelta {
    /// New attributed messages in chronological order.
    pub messages: Vec<SessionMessage>,
    /// Progress valid only after the host accepts this delta and its trigger.
    pub next_state: SharingState,
}

/// Why incremental sharing must be replaced by a full P4 initialization.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReinitializeReason {
    /// The requested, bound, or stored conversation differs.
    ConversationChanged,
    /// The watermark was not crossed inside the bounded raw-row scan.
    GapTooLarge,
    /// The host log ended before reaching the watermark.
    WatermarkUnavailable,
}

/// Result of preparing continuous transcript sharing for one turn.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SharingPlan {
    /// Apply these additions, then commit the included state after acceptance.
    Delta(SessionDelta),
    /// Discard incremental planning and perform full P4 initialization.
    Reinitialize {
        /// Precise reason a new initialization is required.
        reason: ReinitializeReason,
    },
}
