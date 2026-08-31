//! Validated projection over a host-owned session log.

#[cfg(test)]
mod test;

mod types;

pub use types::{
    Conversation, LogMessage, Sequence, SessionAuthor, SessionMessage, SessionPage, SessionQuery,
};

use crate::{Error, Result};
use std::{error::Error as StdError, future::Future, pin::Pin};
use tinyteams_core::chat::same_conversation;

/// Default number of qualifying messages requested for a turn.
pub const SESSION_WINDOW: usize = 30;
/// Maximum raw rows requested in one host read.
pub const PAGE_SIZE: usize = 512;
/// Maximum raw rows inspected during one projection.
pub const SCAN_LIMIT: usize = 2048;

/// A boxed source error returned by a host log implementation.
pub type SourceError = Box<dyn StdError + Send + Sync + 'static>;

/// The boxed future returned by [`SessionLog`].
pub type SessionFuture<'a> =
    Pin<Box<dyn Future<Output = std::result::Result<SessionPage, SourceError>> + Send + 'a>>;

/// Read-only access to the host's append-only session log.
///
/// Implementations must return rows newest-first and treat `before` as an
/// exclusive sequence bound. The trait is object-safe and chooses no executor.
pub trait SessionLog: Send + Sync {
    /// Read at most `limit` raw rows older than `before`.
    fn read_before(&self, before: Option<Sequence>, limit: usize) -> SessionFuture<'_>;
}

/// Project one bounded, attributed history from a host log.
///
/// Reaching [`SCAN_LIMIT`] is a successful partial projection. The returned
/// messages are chronological even though the port pages newest-first.
///
/// # Errors
///
/// Returns [`Error::Read`] for a host read failure or a typed page-validation
/// error when a host violates ordering, uniqueness, size, or cursor contracts.
pub async fn project_session(
    log: &(dyn SessionLog + '_),
    query: &SessionQuery,
) -> Result<Vec<SessionMessage>> {
    if query.window == 0 {
        return Ok(Vec::new());
    }

    let mut cursor = query.before;
    let mut scanned = 0_usize;
    let mut seen = Vec::new();
    let mut projected = Vec::new();
    let mut reached_root = false;

    while scanned < SCAN_LIMIT && projected.len() < query.window && !reached_root {
        let limit = PAGE_SIZE.min(SCAN_LIMIT - scanned);
        let page = log
            .read_before(cursor, limit)
            .await
            .map_err(|source| Error::Read { source })?;
        validate_page(&page, cursor, limit, &mut seen)?;
        scanned += page.messages.len();

        for message in &page.messages {
            if !matches_conversation(message, &query.conversation) {
                continue;
            }
            let is_thread_root = query.conversation.thread_root == Some(message.sequence);
            if message.content.trim().is_empty() {
                if is_thread_root {
                    reached_root = true;
                    break;
                }
                continue;
            }
            projected.push(SessionMessage {
                sequence: message.sequence,
                author: message.author.clone(),
                content: message.content.clone(),
            });
            if is_thread_root {
                reached_root = true;
                break;
            }
            if projected.len() == query.window {
                break;
            }
        }

        if reached_root || projected.len() == query.window || scanned == SCAN_LIMIT {
            break;
        }
        let Some(next) = page.next_before else {
            break;
        };
        cursor = Some(next);
    }

    projected.truncate(query.window);
    projected.reverse();
    Ok(projected)
}

pub(crate) fn validate_page(
    page: &SessionPage,
    before: Option<Sequence>,
    requested: usize,
    seen: &mut Vec<Sequence>,
) -> Result<()> {
    if page.messages.len() > requested {
        return Err(Error::PageTooLarge {
            requested,
            actual: page.messages.len(),
        });
    }
    if page.messages.is_empty() {
        return match page.next_before {
            Some(next_before) => Err(Error::EmptyPageCursor { next_before }),
            None => Ok(()),
        };
    }

    for (index, message) in page.messages.iter().enumerate() {
        if seen.contains(&message.sequence) {
            return Err(Error::DuplicateSequence {
                sequence: message.sequence,
            });
        }
        if let Some(bound) = before
            && message.sequence >= bound
        {
            return Err(Error::PageOutOfRange {
                sequence: message.sequence,
                before: bound,
            });
        }
        if let Some(previous) = index
            .checked_sub(1)
            .map(|prior| page.messages[prior].sequence)
            && previous <= message.sequence
        {
            return Err(Error::PageNotDescending {
                previous,
                next: message.sequence,
            });
        }
        seen.push(message.sequence);
    }

    if let Some(next_before) = page.next_before {
        if let Some(bound) = before
            && next_before >= bound
        {
            return Err(Error::CursorDidNotAdvance {
                before: bound,
                next_before,
            });
        }
        let oldest = page.messages[page.messages.len() - 1].sequence;
        if next_before > oldest {
            return Err(Error::CursorAfterOldest {
                next_before,
                oldest,
            });
        }
    }
    Ok(())
}

pub(crate) fn matches_conversation(message: &LogMessage, conversation: &Conversation) -> bool {
    let matches_desk = same_conversation(message.chat_id.as_deref(), Some(&conversation.desk_id))
        || same_conversation(message.chat_id.as_deref(), Some(&conversation.desk_name));
    if !matches_desk {
        return false;
    }
    match conversation.thread_root {
        None => message.parent.is_none(),
        Some(root) => message.sequence == root || message.parent == Some(root),
    }
}
