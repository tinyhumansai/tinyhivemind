//! Ranking a query against borrowed candidates, once, for every picker.
//!
//! Agents, desks, threads and pinned messages are all selected the same way:
//! a short query, a bounded list of candidates, and a deterministic order over
//! the ones that matched. Writing that ordering once means an agent search and
//! a desk search cannot disagree about whether a prefix beats a substring, and
//! it means the ordering itself is a fold a test can pin exactly.
//!
//! Scoring is fixed-point integer arithmetic — a tier base plus a density
//! term — so every score is reproducible across machines and every payload
//! here derives `Eq`.
//!
//! # Example
//!
//! ```
//! use tinyhivemind_core::select::{Candidate, MatchKind, rank};
//!
//! let candidates = [
//!     Candidate::new("alice", "Alice"),
//!     Candidate::new("alistair", "Alistair"),
//!     Candidate::new("bob", "Bob").with_detail("reviews Alice's work"),
//! ];
//! let hits = rank("ali", &candidates, 8);
//! assert_eq!(hits[0].id, "alice");
//! assert_eq!(hits[0].kind, MatchKind::Prefix);
//! // The detail match is last: supporting text is scored at half weight.
//! assert_eq!(hits[2].id, "bob");
//! ```

#[cfg(test)]
mod test;

mod types;

pub use types::{Candidate, Hit, MatchField, MatchKind, Pattern, TextMatch};

/// Default number of hits a picker offers.
///
/// A selection list exists to be read by whoever asked, and a list longer than
/// this stops being a choice and becomes a second search.
pub const SELECT_LIMIT: usize = 8;

/// Largest density bonus a match can earn, in fixed-point units.
pub const DENSITY_SCALE: u32 = 100;

/// Score one query against one piece of text, or `None` when it does not match.
///
/// Both sides are trimmed and lowercased first, so matching is
/// case-insensitive and offsets are character offsets into the lowercased
/// text. A blank query matches nothing: an empty picker query is a request to
/// list, and listing is the caller's own snapshot to iterate.
///
/// The score is the [`MatchKind::base`] of the tier plus a density term,
/// `DENSITY_SCALE * query_chars / text_chars`, which prefers the shorter of
/// two candidates that matched the same way.
#[must_use]
pub fn score(query: &str, text: &str) -> Option<TextMatch> {
    let needle: Vec<char> = query.trim().to_lowercase().chars().collect();
    let haystack: Vec<char> = text.trim().to_lowercase().chars().collect();
    if needle.is_empty() || haystack.is_empty() || needle.len() > haystack.len() {
        return None;
    }

    let (kind, offset) = if needle == haystack {
        (MatchKind::Exact, 0)
    } else if haystack.starts_with(&needle) {
        (MatchKind::Prefix, 0)
    } else if let Some(offset) = occurrence(&haystack, &needle) {
        let kind = if starts_word(&haystack, offset) {
            MatchKind::WordPrefix
        } else {
            MatchKind::Substring
        };
        (kind, offset)
    } else {
        let offset = subsequence(&haystack, &needle)?;
        (MatchKind::Subsequence, offset)
    };

    Some(scored(kind, offset, needle.len(), haystack.len()))
}

/// Score one pattern against one piece of text, or `None` when it misses.
///
/// A [`Pattern::Text`] scores exactly as [`score`] does. A
/// [`Pattern::Regex`] is matched against the trimmed text as written — case
/// folding is the expression's own business, spelled `(?i)` — and the span
/// the engine returns is read onto the same tiers: a span covering the whole
/// text is [`MatchKind::Exact`], one at offset zero is
/// [`MatchKind::Prefix`], one starting a word is [`MatchKind::WordPrefix`],
/// and anything else is [`MatchKind::Substring`]. Density is the share of the
/// text the span covers, so a regular-expression hit and a literal hit are
/// comparable in one ranked list.
#[must_use]
pub fn score_pattern(pattern: &Pattern<'_>, text: &str) -> Option<TextMatch> {
    match pattern {
        Pattern::Text(query) => score(query, text),
        #[cfg(feature = "regex")]
        Pattern::Regex(expression) => score_regex(expression, text),
    }
}

#[cfg(feature = "regex")]
fn score_regex(expression: &regex::Regex, text: &str) -> Option<TextMatch> {
    let text = text.trim();
    let found = expression.find(text)?;
    // A zero-width match — `^`, a lookaround-free `\b`, an all-optional
    // pattern — says the text satisfied the expression without naming any of
    // it. That is a match, and it is the weakest one there is: no span means
    // no density and no tier above a substring.
    if found.is_empty() {
        return Some(TextMatch {
            kind: MatchKind::Substring,
            score: MatchKind::Substring.base(),
            offset: text[..found.start()].chars().count(),
        });
    }
    let haystack: Vec<char> = text.chars().collect();
    let offset = text[..found.start()].chars().count();
    let length = found.as_str().chars().count();
    let kind = if length == haystack.len() {
        MatchKind::Exact
    } else if offset == 0 {
        MatchKind::Prefix
    } else if starts_word(&haystack, offset) {
        MatchKind::WordPrefix
    } else {
        MatchKind::Substring
    };
    Some(scored(kind, offset, length, haystack.len()))
}

/// Assemble one scored match from its tier and the span it covered.
fn scored(kind: MatchKind, offset: usize, matched: usize, whole: usize) -> TextMatch {
    TextMatch {
        kind,
        score: kind.base() + ratio(matched, whole),
        offset,
    }
}

/// Read the source of a `/…/`-delimited regular-expression query.
///
/// A picker has one input box, so the intent to search by expression has to
/// be spelled inside the query itself. This is that spelling, and it is pure:
/// it recognises the delimiters and hands back what is between them, leaving
/// compilation — with whatever syntax, flags and size limits the host has
/// decided on — to the caller.
///
/// ```
/// use tinyhivemind_core::select::regex_source;
///
/// assert_eq!(regex_source("/^ship(ped)?/"), Some("^ship(ped)?"));
/// assert_eq!(regex_source("shipped"), None);
/// // An empty expression is not one.
/// assert_eq!(regex_source("//"), None);
/// ```
#[must_use]
pub fn regex_source(query: &str) -> Option<&str> {
    let trimmed = query.trim();
    let inner = trimmed.strip_prefix('/')?.strip_suffix('/')?;
    (!inner.is_empty()).then_some(inner)
}

/// Rank candidates against a query, best first, keeping at most `limit`.
///
/// A candidate is scored on its label and its id at full weight and on its
/// detail at half, and reported under whichever piece scored highest — a tie
/// resolving to the label, then the id. Hits are ordered by score, then by the
/// earlier match, then by the shorter label, and finally by candidate order,
/// which is stable and total: the same snapshot and query always produce the
/// same list.
#[must_use]
pub fn rank<'a>(query: &str, candidates: &[Candidate<'a>], limit: usize) -> Vec<Hit<'a>> {
    rank_pattern(&Pattern::Text(query), candidates, limit)
}

/// Rank candidates against a pattern, best first, keeping at most `limit`.
///
/// Identical to [`rank`] in every respect but what a candidate is matched
/// against; see [`score_pattern`].
#[must_use]
pub fn rank_pattern<'a>(
    pattern: &Pattern<'_>,
    candidates: &[Candidate<'a>],
    limit: usize,
) -> Vec<Hit<'a>> {
    if limit == 0 {
        return Vec::new();
    }
    let mut hits: Vec<Hit<'a>> = candidates
        .iter()
        .filter_map(|candidate| best_field(pattern, candidate))
        .collect();
    hits.sort_by_key(|hit| {
        (
            std::cmp::Reverse(hit.score),
            hit.offset,
            hit.label.chars().count(),
        )
    });
    hits.truncate(limit);
    hits
}

/// Score every piece of one candidate and keep the strongest.
fn best_field<'a>(pattern: &Pattern<'_>, candidate: &Candidate<'a>) -> Option<Hit<'a>> {
    let mut best: Option<(MatchField, TextMatch)> = None;
    let fields = [
        (MatchField::Label, score_pattern(pattern, candidate.label)),
        (MatchField::Id, score_pattern(pattern, candidate.id)),
        (
            MatchField::Detail,
            candidate
                .detail
                .and_then(|detail| score_pattern(pattern, detail))
                .map(halve),
        ),
    ];
    for (field, matched) in fields {
        let Some(matched) = matched else { continue };
        let better = match &best {
            None => true,
            Some((best_field, best_match)) => {
                (matched.score, field) > (best_match.score, *best_field)
            }
        };
        if better {
            best = Some((field, matched));
        }
    }
    let (field, matched) = best?;
    Some(Hit {
        id: candidate.id,
        label: candidate.label,
        field,
        kind: matched.kind,
        score: matched.score,
        offset: matched.offset,
    })
}

/// Halve a detail match's score, keeping its tier and offset.
///
/// Supporting text is evidence that a candidate is *related* to the query, not
/// that it is named by it. Halving rather than dropping a tier keeps a strong
/// detail match ahead of a weak name match without ever letting a description
/// outrank the thing the query actually spelled.
const fn halve(matched: TextMatch) -> TextMatch {
    TextMatch {
        kind: matched.kind,
        score: matched.score / 2,
        offset: matched.offset,
    }
}

/// Offset of the best verbatim occurrence: the first at a word boundary,
/// otherwise the first anywhere.
fn occurrence(haystack: &[char], needle: &[char]) -> Option<usize> {
    let mut first = None;
    for offset in 0..=haystack.len() - needle.len() {
        if !haystack[offset..].starts_with(needle) {
            continue;
        }
        if starts_word(haystack, offset) {
            return Some(offset);
        }
        if first.is_none() {
            first = Some(offset);
        }
    }
    first
}

/// Whether the character before `offset` ends a word.
fn starts_word(haystack: &[char], offset: usize) -> bool {
    offset
        .checked_sub(1)
        .is_none_or(|previous| !haystack[previous].is_alphanumeric())
}

/// Offset of the first character of a greedy in-order subsequence match.
fn subsequence(haystack: &[char], needle: &[char]) -> Option<usize> {
    let mut first = None;
    let mut wanted = needle.iter();
    let mut next = wanted.next()?;
    for (offset, character) in haystack.iter().enumerate() {
        if character != next {
            continue;
        }
        if first.is_none() {
            first = Some(offset);
        }
        match wanted.next() {
            Some(further) => next = further,
            None => return first,
        }
    }
    None
}

/// `DENSITY_SCALE * part / whole`, saturating, with no floating point.
///
/// `whole` is the character count of a text that already matched, so it is
/// never zero; the `max(1)` is there so this cannot divide by zero even if a
/// future caller forgets that, rather than as a branch nothing reaches.
fn ratio(part: usize, whole: usize) -> u32 {
    let scaled = part.saturating_mul(DENSITY_SCALE as usize) / whole.max(1);
    u32::try_from(scaled)
        .unwrap_or(DENSITY_SCALE)
        .min(DENSITY_SCALE)
}
