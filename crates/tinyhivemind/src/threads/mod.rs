//! A recency-ordered index of the live threads in one desk.

#[cfg(test)]
mod test;

mod types;

pub use types::ThreadLine;

use crate::{
    Conversation, Error, LogMessage, PAGE_SIZE, Result, Sequence, SessionLog,
    session::{in_desk, validate_page},
};
use std::collections::BTreeMap;

/// Default number of threads described to a viewer.
pub const THREAD_INDEX_LIMIT: usize = 5;
/// Characters of a root kept as its opening words.
pub const THREAD_OPENING_CHARS: usize = 60;
/// Maximum raw rows inspected while building one index.
///
/// Deliberately far below [`crate::SCAN_LIMIT`]. The index answers "what is
/// live here", which is a recency question, and paying a full scan to surface a
/// thread nobody has touched in two thousand messages is the wrong trade. A
/// thread whose root fell outside this bound is absent from the index even when
/// its replies are recent, because a reply alone cannot supply an opening.
pub const THREAD_INDEX_SCAN: usize = 256;

/// Read a bounded, recency-ordered index of the threads in a desk.
///
/// Returns nothing for a thread-scoped conversation: a viewer already inside a
/// thread is not choosing between them. Returns nothing for a zero limit, and
/// performs no read in either case.
///
/// # Errors
///
/// Returns [`Error::Read`] for a host read failure or a typed page-validation
/// error when a host violates ordering, uniqueness, size, or cursor contracts.
pub async fn read_thread_index(
    log: &(dyn SessionLog + '_),
    conversation: &Conversation,
    limit: usize,
) -> Result<Vec<ThreadLine>> {
    if limit == 0 || conversation.thread_root.is_some() {
        return Ok(Vec::new());
    }

    let mut cursor = None;
    let mut scanned = 0_usize;
    let mut seen = Vec::new();
    let mut rows: Vec<LogMessage> = Vec::new();

    while scanned < THREAD_INDEX_SCAN {
        let read = PAGE_SIZE.min(THREAD_INDEX_SCAN - scanned);
        let page = log
            .read_before(cursor, read)
            .await
            .map_err(|source| Error::Read { source })?;
        validate_page(&page, cursor, read, &mut seen)?;
        scanned += page.messages.len();

        rows.extend(
            page.messages
                .iter()
                .filter(|message| in_desk(message, conversation))
                .cloned(),
        );

        if scanned >= THREAD_INDEX_SCAN {
            break;
        }
        let Some(next) = page.next_before else {
            break;
        };
        cursor = Some(next);
    }

    rows.reverse();
    Ok(fold_thread_index(&rows, limit))
}

/// Fold a chronological slice of one desk's rows into a thread index.
///
/// A root with no readable opening is not indexed, and neither are its replies:
/// the row exists to say what a thread is about, and a blank one says nothing.
/// A reply whose root is not in the slice is ignored for the same reason.
#[must_use]
pub fn fold_thread_index(rows: &[LogMessage], limit: usize) -> Vec<ThreadLine> {
    let mut threads: BTreeMap<Sequence, ThreadLine> = BTreeMap::new();

    for row in rows {
        let content = row.content.trim();
        match row.parent {
            None => {
                if content.is_empty() {
                    continue;
                }
                threads.insert(
                    row.sequence,
                    ThreadLine {
                        root: row.sequence,
                        opening: opening_words(content),
                        replies: 0,
                        latest: row.sequence,
                        landed: None,
                    },
                );
            }
            Some(parent) => {
                if content.is_empty() {
                    continue;
                }
                if let Some(thread) = threads.get_mut(&parent) {
                    thread.replies += 1;
                    thread.latest = row.sequence;
                }
            }
        }
    }

    let mut index: Vec<ThreadLine> = threads.into_values().collect();
    // A sequence belongs to exactly one thread, so no two rows share a
    // `latest` and this ordering is already total. The sort is stable, so the
    // map's root-ascending order would decide a tie that cannot occur.
    index.sort_by_key(|line| std::cmp::Reverse(line.latest));
    index.truncate(limit);
    index
}

/// Take the opening words of a root, cut on a character boundary.
fn opening_words(content: &str) -> String {
    let single_line: String = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if single_line.chars().count() <= THREAD_OPENING_CHARS {
        return single_line;
    }
    let mut opening: String = single_line.chars().take(THREAD_OPENING_CHARS).collect();
    opening.push('…');
    opening
}
