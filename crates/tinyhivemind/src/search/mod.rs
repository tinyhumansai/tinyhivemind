//! Searching the shared transcript: messages, and the threads that hold them.
//!
//! An agent's turn sees a bounded window of the desk — thirty messages, by
//! default — and the log behind it is unbounded. Search is how the rest of it
//! stays reachable: instead of enlarging the window until it stops fitting,
//! the transcript becomes something a turn *queries*, and only what matched
//! comes back. [Pinning](crate::pins) is the other half of the same answer,
//! for the handful of insights that must ride along on every turn whether or
//! not anybody thought to search for them.
//!
//! Ranking is [`tinyhivemind_core::select`], the same ordering the agent and
//! desk pickers use, so a literal query and a regular expression land in one
//! comparable list.
//!
//! # Bounds
//!
//! A search reads at most [`SEARCH_SCAN`] raw rows through the same
//! [`SessionLog`] port everything else uses, and returns at most
//! `SearchQuery::limit` hits. Reaching the scan bound is a successful partial
//! search over the newest rows, not an error — the same contract
//! [`project_session`](crate::project_session) has.

#[cfg(test)]
mod test;

mod types;

pub use types::{MessageHit, SearchPattern, SearchQuery, ThreadHit};

use crate::{
    Conversation, Error, LogMessage, PAGE_SIZE, Result, SessionAuthor, SessionLog,
    session::{in_desk, matches_conversation, validate_page},
    threads::{THREAD_INDEX_SCAN, fold_thread_index, read_desk_rows},
};
use tinyhivemind_core::select::{Pattern, TextMatch, score_pattern};

/// Default number of hits one search returns.
pub const SEARCH_LIMIT: usize = 10;
/// Maximum raw rows inspected during one search.
pub const SEARCH_SCAN: usize = 2048;
/// Characters of a matching row kept as its excerpt.
pub const EXCERPT_CHARS: usize = 96;
/// Characters of context kept before the match inside an excerpt.
const EXCERPT_LEAD: usize = 24;

/// Search the host log for messages matching a query.
///
/// Hits are ordered by score, then newest first, and truncated to the query's
/// limit. An empty text query matches nothing and performs no read: an empty
/// picker box is not a request for the whole log.
///
/// # Errors
///
/// Returns [`Error::RegexUnsupported`] when a
/// [`SearchPattern::Regex`] is used without the `regex` feature,
/// [`Error::InvalidPattern`] when its source does not compile, [`Error::Read`]
/// for a host read failure, or a typed page-validation error when a host
/// violates ordering, uniqueness, size, or cursor contracts.
pub async fn search_messages(
    log: &(dyn SessionLog + '_),
    query: &SearchQuery,
) -> Result<Vec<MessageHit>> {
    if query.limit == 0 || is_blank(&query.pattern) {
        return Ok(Vec::new());
    }
    let compiled = CompiledPattern::compile(&query.pattern)?;
    let pattern = compiled.pattern();

    let mut cursor = query.before;
    let mut scanned = 0_usize;
    let mut seen = Vec::new();
    let mut hits: Vec<MessageHit> = Vec::new();

    while scanned < SEARCH_SCAN {
        let read = PAGE_SIZE.min(SEARCH_SCAN - scanned);
        let page = log
            .read_before(cursor, read)
            .await
            .map_err(|source| Error::Read { source })?;
        validate_page(&page, cursor, read, &mut seen)?;
        scanned += page.messages.len();

        for message in &page.messages {
            if !in_scope(message, query.scope.as_ref()) || !by_author(message, query) {
                continue;
            }
            let line = collapse(&message.content);
            let Some(matched) = score_pattern(&pattern, &line) else {
                continue;
            };
            hits.push(hit(message, &line, &matched));
        }

        if scanned >= SEARCH_SCAN {
            break;
        }
        let Some(next) = page.next_before else {
            break;
        };
        cursor = Some(next);
    }

    // The walk is newest-first, so equal scores already arrive newest-first
    // and a stable sort by score alone preserves that.
    hits.sort_by_key(|hit| std::cmp::Reverse(hit.score));
    hits.truncate(query.limit);
    Ok(hits)
}

/// Search one desk's threads by their opening words.
///
/// Bounded by [`THREAD_INDEX_SCAN`] rather than [`SEARCH_SCAN`], for the
/// reason the thread index is: which threads are live is a recency question.
/// A thread-scoped conversation searches nothing and reads nothing — a viewer
/// already inside a thread is not choosing between them.
///
/// # Errors
///
/// Returns the errors documented by [`search_messages`].
pub async fn search_threads(
    log: &(dyn SessionLog + '_),
    conversation: &Conversation,
    pattern: &SearchPattern,
    limit: usize,
) -> Result<Vec<ThreadHit>> {
    if limit == 0 || conversation.thread_root.is_some() || is_blank(pattern) {
        return Ok(Vec::new());
    }
    let compiled = CompiledPattern::compile(pattern)?;
    let rows = read_desk_rows(log, conversation, THREAD_INDEX_SCAN).await?;
    Ok(rank_threads(&rows, &compiled.pattern(), limit))
}

/// Rank every thread in a chronological desk slice against one pattern.
fn rank_threads(rows: &[LogMessage], pattern: &Pattern<'_>, limit: usize) -> Vec<ThreadHit> {
    let index = fold_thread_index(rows, usize::MAX);
    let mut hits: Vec<ThreadHit> = index
        .into_iter()
        .filter_map(|line| {
            let matched = score_pattern(pattern, &line.opening)?;
            Some(ThreadHit {
                line,
                score: matched.score,
                kind: matched.kind,
            })
        })
        .collect();
    // `fold_thread_index` already ordered by latest activity, and the sort is
    // stable, so a tied score keeps the livelier thread first.
    hits.sort_by_key(|hit| std::cmp::Reverse(hit.score));
    hits.truncate(limit);
    hits
}

/// Whether a pattern can match anything at all.
fn is_blank(pattern: &SearchPattern) -> bool {
    match pattern {
        SearchPattern::Text { query } => query.trim().is_empty(),
        SearchPattern::Regex { source } => source.is_empty(),
    }
}

/// A pattern the search owns for the duration of one walk.
enum CompiledPattern {
    Text(String),
    #[cfg(feature = "regex")]
    Regex(Box<regex::Regex>),
}

impl CompiledPattern {
    fn compile(pattern: &SearchPattern) -> Result<Self> {
        match pattern {
            SearchPattern::Text { query } => Ok(Self::Text(query.clone())),
            #[cfg(feature = "regex")]
            SearchPattern::Regex { source } => match regex::Regex::new(source) {
                Ok(expression) => Ok(Self::Regex(Box::new(expression))),
                Err(error) => Err(Error::InvalidPattern {
                    pattern: source.clone(),
                    message: error.to_string(),
                }),
            },
            #[cfg(not(feature = "regex"))]
            SearchPattern::Regex { source } => Err(Error::RegexUnsupported {
                pattern: source.clone(),
            }),
        }
    }

    fn pattern(&self) -> Pattern<'_> {
        match self {
            Self::Text(query) => Pattern::Text(query.as_str()),
            #[cfg(feature = "regex")]
            Self::Regex(expression) => Pattern::Regex(expression.as_ref()),
        }
    }
}

/// Whether a row is inside the searched desk or thread.
fn in_scope(message: &LogMessage, scope: Option<&Conversation>) -> bool {
    match scope {
        None => true,
        Some(conversation) if conversation.thread_root.is_some() => {
            matches_conversation(message, conversation)
        }
        Some(conversation) => in_desk(message, conversation),
    }
}

/// Whether a row was written by the requested author id.
fn by_author(message: &LogMessage, query: &SearchQuery) -> bool {
    let Some(wanted) = query.author_id.as_deref() else {
        return true;
    };
    match &message.author {
        SessionAuthor::Agent { id, .. } | SessionAuthor::Person { id, .. } => id == wanted,
        SessionAuthor::System { kind, .. } => kind == wanted,
        SessionAuthor::Operator => wanted == "operator",
    }
}

/// Build one hit from a matching row and the collapsed line that matched.
fn hit(message: &LogMessage, line: &str, matched: &TextMatch) -> MessageHit {
    MessageHit {
        sequence: message.sequence,
        chat_id: message.chat_id.clone(),
        parent: message.parent,
        author: message.author.clone(),
        excerpt: excerpt(line, matched.offset),
        score: matched.score,
        kind: matched.kind,
    }
}

/// Collapse a row to one line, so an excerpt is a line and offsets are stable.
fn collapse(content: &str) -> String {
    content.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Take a window of the collapsed line around the match, cut on characters.
///
/// The offset is a character index into the *lowercased* line, which for
/// almost every input is the same index as in the line itself. Where a
/// lowercasing changes a character count the window simply lands a character
/// or two off; every cut here saturates, so it can never panic or split a
/// character.
fn excerpt(line: &str, offset: usize) -> String {
    let characters: Vec<char> = line.chars().collect();
    if characters.len() <= EXCERPT_CHARS {
        return line.to_owned();
    }
    let start = offset.saturating_sub(EXCERPT_LEAD).min(
        characters
            .len()
            .saturating_sub(EXCERPT_CHARS)
            .min(characters.len()),
    );
    let end = (start + EXCERPT_CHARS).min(characters.len());
    let mut excerpt = String::new();
    if start > 0 {
        excerpt.push('…');
    }
    excerpt.extend(&characters[start..end]);
    if end < characters.len() {
        excerpt.push('…');
    }
    excerpt
}
