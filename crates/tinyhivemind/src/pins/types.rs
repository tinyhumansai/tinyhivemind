//! Stable host-facing pin records.

use crate::{Sequence, SessionAuthor};
use serde::{Deserialize, Serialize};

/// What one pin marker does to the board.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PinAction {
    /// Put a message on the board, or update the one already there.
    Pin,
    /// Take a message off the board.
    Unpin,
}

/// One pin marker read out of an authored message.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PinDirective {
    /// Sequence of the message that carried the marker.
    pub sequence: Sequence,
    /// Message the marker acts on; the carrier itself when none was named.
    pub target: Sequence,
    /// Preserved author of the carrying message.
    pub author: SessionAuthor,
    /// Whether the marker pins or unpins.
    pub action: PinAction,
    /// Short `#label` grouping the pin, when the marker carried one.
    pub label: Option<String>,
    /// Free text after the marker's arguments, when it carried any.
    pub note: Option<String>,
}

/// One message held on a conversation's board.
///
/// A pin is a fold over the transcript, not a record beside it: everything
/// here was read back out of the log, which is why there is no second journal
/// to keep consistent with the first.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct Pin {
    /// The pinned message.
    pub sequence: Sequence,
    /// Sequence of the marker that most recently pinned it.
    pub pinned_at: Sequence,
    /// Who pinned it.
    pub pinned_by: SessionAuthor,
    /// Short `#label` grouping the pin, when one was given.
    pub label: Option<String>,
    /// Why it was pinned, when the marker said so.
    pub note: Option<String>,
    /// Opening words of the pinned message, when the fold saw it.
    ///
    /// `None` when the pinned message fell outside the scanned rows: the board
    /// still knows the sequence, so a host can read that one row directly.
    pub excerpt: Option<String>,
}
