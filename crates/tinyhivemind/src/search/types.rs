//! Stable host-facing search records.

use crate::{Conversation, Sequence, SessionAuthor, ThreadLine};
use serde::{Deserialize, Serialize};
use tinyhivemind_core::select::MatchKind;

/// What a search matches rows against.
///
/// This is the wire form of a pattern, not a compiled one: a host stores and
/// forwards a search, and a compiled expression is neither serializable nor
/// safe to hold across a process boundary. Compilation happens once inside the
/// search, under the caller's `regex` feature.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SearchPattern {
    /// A literal query, matched case-insensitively and ranked by tier.
    Text {
        /// The authored query.
        query: String,
    },
    /// A regular-expression source, compiled by the search.
    ///
    /// Requires the crate's `regex` feature; without it a search carrying one
    /// fails with [`Error::RegexUnsupported`](crate::Error::RegexUnsupported)
    /// rather than silently falling back to a literal search for the source
    /// text, which would return confidently wrong results.
    Regex {
        /// The expression source, without delimiters.
        source: String,
    },
}

impl SearchPattern {
    /// Read a picker query, treating a `/…/`-delimited one as an expression.
    ///
    /// This is the single-input-box spelling: what a person types into one
    /// field decides which of the two searches they get.
    #[must_use]
    pub fn parse(query: &str) -> Self {
        match tinyhivemind_core::select::regex_source(query) {
            Some(source) => Self::Regex {
                source: source.to_owned(),
            },
            None => Self::Text {
                query: query.trim().to_owned(),
            },
        }
    }
}

/// Parameters for one bounded transcript search.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SearchQuery {
    /// What rows are matched against.
    pub pattern: SearchPattern,
    /// Desk or thread to search, or `None` for every desk in the log.
    ///
    /// A desk-scoped search reads the desk's whole interior, thread replies
    /// included. That is the difference between search and projection: the
    /// projection is bounded so a turn stays readable, and the search exists
    /// precisely to reach the reply buried three deep in an old thread.
    pub scope: Option<Conversation>,
    /// Keep only rows written by this agent or person id, when set.
    pub author_id: Option<String>,
    /// Exclusive upper sequence bound, often the triggering message.
    pub before: Option<Sequence>,
    /// Maximum number of hits returned.
    pub limit: usize,
}

impl SearchQuery {
    /// A whole-log search for a picker query, with the default limit.
    #[must_use]
    pub fn new(query: &str) -> Self {
        Self {
            pattern: SearchPattern::parse(query),
            scope: None,
            author_id: None,
            before: None,
            limit: super::SEARCH_LIMIT,
        }
    }

    /// Restrict the search to one desk or thread.
    #[must_use]
    pub fn in_conversation(mut self, conversation: Conversation) -> Self {
        self.scope = Some(conversation);
        self
    }

    /// Restrict the search to one author id.
    #[must_use]
    pub fn by_author(mut self, author_id: impl Into<String>) -> Self {
        self.author_id = Some(author_id.into());
        self
    }
}

/// One matching message, with enough of it to decide whether to go read it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct MessageHit {
    /// Host sequence of the matching row, and its address for a follow-up read.
    pub sequence: Sequence,
    /// Stored desk spelling of the row; `None` is General.
    pub chat_id: Option<String>,
    /// Direct parent sequence when the row is a thread reply.
    pub parent: Option<Sequence>,
    /// Preserved author of the row.
    pub author: SessionAuthor,
    /// Whitespace-collapsed window of the row around the match.
    pub excerpt: String,
    /// Fixed-point score that ordered this hit.
    pub score: u32,
    /// Tier the match fell in.
    pub kind: MatchKind,
}

/// One matching thread, summarised exactly as the thread index summarises it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ThreadHit {
    /// The thread, as [`fold_thread_index`](crate::fold_thread_index) folds it.
    pub line: ThreadLine,
    /// Fixed-point score that ordered this hit.
    pub score: u32,
    /// Tier the match fell in.
    pub kind: MatchKind,
}
