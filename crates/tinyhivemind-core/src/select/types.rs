//! Stable values describing one ranked selection.

use serde::{Deserialize, Serialize};

/// How a query matched one piece of candidate text, worst kind first.
///
/// The ordering is the ranking: a prefix beats a substring, and an exact
/// match beats everything. Deriving `Ord` here rather than comparing scores
/// keeps the tiers separable in a test.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchKind {
    /// Every query character appears in order, with other characters between.
    Subsequence,
    /// The query appears verbatim, starting inside a word.
    Substring,
    /// The query appears verbatim, starting at a word boundary.
    WordPrefix,
    /// The candidate text starts with the query.
    Prefix,
    /// The candidate text is the query.
    Exact,
}

impl MatchKind {
    /// The tier's base score, before density is added.
    ///
    /// Tiers are two hundred points apart and density contributes at most a
    /// hundred, so no amount of density promotes a weaker kind past a
    /// stronger one. That is the whole point of a fixed-point score: the
    /// ordering is decided by the grammar of the match, not by a weight
    /// somebody tuned.
    #[must_use]
    pub const fn base(self) -> u32 {
        match self {
            Self::Subsequence => 200,
            Self::Substring => 400,
            Self::WordPrefix => 600,
            Self::Prefix => 800,
            Self::Exact => 1000,
        }
    }
}

/// Which piece of a candidate produced the match.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchField {
    /// The candidate's supporting text, scored at half weight.
    Detail,
    /// The candidate's opaque identifier.
    Id,
    /// The candidate's display label.
    Label,
}

/// One scored match of a query against a single piece of text.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct TextMatch {
    /// The tier this match fell in.
    pub kind: MatchKind,
    /// Tier base plus density, in fixed-point units.
    pub score: u32,
    /// Character offset of the match in the lowercased text.
    pub offset: usize,
}

/// One thing a query may select.
///
/// This is a borrowed call-only view, like [`Roster`](crate::roster::Roster),
/// and intentionally has no serde representation: what a host stores is the
/// record the candidate was built from, never the candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Candidate<'a> {
    /// Opaque, case-sensitive identifier returned on a hit.
    pub id: &'a str,
    /// Display label, matched at full weight.
    pub label: &'a str,
    /// Supporting text — a description, an opening line — at half weight.
    pub detail: Option<&'a str>,
}

impl<'a> Candidate<'a> {
    /// Build a candidate whose label is also its identifier.
    #[must_use]
    pub const fn new(id: &'a str, label: &'a str) -> Self {
        Self {
            id,
            label,
            detail: None,
        }
    }

    /// Attach supporting text matched at half weight.
    #[must_use]
    pub const fn with_detail(mut self, detail: &'a str) -> Self {
        self.detail = Some(detail);
        self
    }
}

/// One ranked candidate, borrowed from the snapshot it was selected out of.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Hit<'a> {
    /// The selected candidate's identifier.
    pub id: &'a str,
    /// The selected candidate's display label.
    pub label: &'a str,
    /// Which piece of the candidate matched.
    pub field: MatchField,
    /// The tier that piece matched in.
    pub kind: MatchKind,
    /// The score that ordered this hit, halved for a detail match.
    pub score: u32,
    /// Character offset of the match within the matched piece.
    pub offset: usize,
}

/// What a selection matches candidates against.
///
/// [`Text`](Self::Text) is the ordinary picker query: case-insensitive, and
/// ranked by the tiers above. [`Regex`](Self::Regex) is for the caller who
/// knows exactly what shape they are looking for and cannot spell it as a
/// substring — it is scored on the same tiers, read off the span the engine
/// matched, so a regular-expression search and a text search rank together.
///
/// The pattern *borrows* a compiled expression rather than a source string:
/// compiling is the caller's decision, and so are the syntax, the flags and
/// the size limits it compiles under.
#[derive(Clone, Copy, Debug)]
pub enum Pattern<'a> {
    /// A literal query, matched case-insensitively.
    Text(&'a str),
    /// A compiled regular expression.
    #[cfg(feature = "regex")]
    Regex(&'a regex::Regex),
}

impl<'a> From<&'a str> for Pattern<'a> {
    fn from(query: &'a str) -> Self {
        Self::Text(query)
    }
}

#[cfg(feature = "regex")]
impl<'a> From<&'a regex::Regex> for Pattern<'a> {
    fn from(expression: &'a regex::Regex) -> Self {
        Self::Regex(expression)
    }
}
