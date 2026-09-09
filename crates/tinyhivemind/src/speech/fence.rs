//! The delimiter a seat writes when it cannot reach the tools.
//!
//! Kept, and kept in one place, for two callers: an agent CLI that will not
//! attach the room's tools, and the tool-less completion a host falls back to
//! when a seat has to speak and must not work — a plain completion has no tool
//! to call.
//!
//! It is here rather than in a host because of what it cost. A model-authored
//! delimiter is an interface with a fallible producer: a seat closed a
//! verified sublinear recursion with `<<<POST>>> … <<<POST>>>` instead of
//! `<<<POST … POST>>>`, the extractor read the last marker as an opener, and
//! the room received the three characters `>>>`. Both spellings are accepted
//! here so that no host has to rediscover the second one.

/// The marker that opens a fenced post, and closes the symmetric spelling.
const OPEN: &str = "<<<POST";

/// The marker that closes the asymmetric spelling.
const CLOSE: &str = "POST>>>";

/// Pull the posted message out of a seat's raw output.
///
/// Returns the whole trimmed text when no fence is present, because a seat
/// that answered without one still said something and losing it is worse than
/// delivering it unfenced.
///
/// Two spellings are accepted: the asymmetric `<<<POST … POST>>>` a brief asks
/// for, and the symmetric `<<<POST>>> … <<<POST>>>` a seat writes anyway. The
/// last opener wins, so a seat that quotes the instruction before obeying it
/// posts what it wrote rather than what it was told.
#[must_use]
pub fn extract_post(text: &str) -> String {
    let opens: Vec<usize> = text.match_indices(OPEN).map(|(at, _)| at).collect();
    let Some(&last) = opens.last() else {
        return text.trim().to_string();
    };
    let after = &text[last + OPEN.len()..];
    // The asymmetric fence: the body runs to the next `POST>>>`.
    if let Some(end) = after.find(CLOSE) {
        return after[..end].trim_start_matches(">>>").trim().to_string();
    }
    // The symmetric one: the last marker is a *closer*, so the body is what
    // sits between it and the marker before it.
    if let Some(&open) = opens.iter().rev().nth(1) {
        let body = &text[open + OPEN.len()..last];
        return body.trim_start_matches(">>>").trim().to_string();
    }
    after.trim_start_matches(">>>").trim().to_string()
}
