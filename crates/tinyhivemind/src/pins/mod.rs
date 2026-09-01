//! Pinning: the small set of messages every turn sees, whatever else it misses.
//!
//! A turn reads a bounded window of a busy desk, so the decision that was
//! settled two hundred messages ago is, by default, gone. [Search](crate::search)
//! makes it *reachable*; a pin makes it *unavoidable*. The two together are why
//! the window can stay small: what matters is either pinned or findable, and
//! everything else is allowed to scroll away.
//!
//! # The grammar
//!
//! A marker is recognised at the start of a line, ignoring leading whitespace,
//! and only outside a fenced code block — the same rule the hive crate's trace
//! grammar uses, and for the same reason.
//!
//! ```text
//! !pin [^N] [#label] [free text]
//! !unpin ^N
//! ```
//!
//! `!pin` with no `^N` pins the message that carries it, which is the common
//! case: an agent marking the insight it just wrote. `!unpin` requires a
//! target, because a marker that removes something must say what.
//!
//! # No second journal
//!
//! The board is a fold over the log. Nothing here is stored, so nothing here
//! can disagree with the transcript it came from, and a host that already
//! keeps the log keeps the pins for free.
//!
//! # Example
//!
//! ```
//! use tinyhivemind::{LogMessage, Sequence, SessionAuthor, pins::fold_pins};
//!
//! let author = SessionAuthor::Agent { id: "alice".into(), label: "Alice".into() };
//! let rows = [
//!     LogMessage {
//!         sequence: Sequence(1),
//!         chat_id: None,
//!         parent: None,
//!         author: author.clone(),
//!         content: "The rate limiter resets at midnight UTC.".into(),
//!     },
//!     LogMessage {
//!         sequence: Sequence(2),
//!         chat_id: None,
//!         parent: None,
//!         author,
//!         content: "!pin ^1 #limits worth remembering".into(),
//!     },
//! ];
//! let board = fold_pins(&rows, 12);
//! assert_eq!(board[0].sequence, Sequence(1));
//! assert_eq!(board[0].label.as_deref(), Some("limits"));
//! assert_eq!(board[0].note.as_deref(), Some("worth remembering"));
//! ```

#[cfg(test)]
mod test;

mod types;

pub use types::{Pin, PinAction, PinDirective};

use crate::{
    BriefingNote, Conversation, LogMessage, Result, Sequence, SessionAuthor, SessionLog,
    session::matches_conversation, threads::read_desk_rows,
};
use std::collections::BTreeMap;

/// Default number of pins a board holds.
///
/// A board is a working set, not an archive. Past a dozen it stops being the
/// thing a turn reads first and becomes a second transcript to skim, which is
/// the problem pinning exists to solve.
pub const PIN_LIMIT: usize = 12;
/// Maximum raw rows inspected while folding one board.
pub const PIN_SCAN: usize = 2048;
/// Characters of a pinned message kept as its excerpt.
pub const PIN_EXCERPT_CHARS: usize = 120;
/// Maximum markers read from one message body.
pub const PIN_MARKER_CAP: usize = 8;

/// Read pin markers from an authored body, in reading order.
///
/// A body carrying no marker yields nothing: ordinary conversation never pins
/// itself by accident.
#[must_use]
pub fn read_directives(
    body: &str,
    author: &SessionAuthor,
    sequence: Sequence,
) -> Vec<PinDirective> {
    if !body.contains('!') {
        return Vec::new();
    }
    let fenced = fenced_ranges(body);
    let mut directives = Vec::new();
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        if fenced
            .iter()
            .any(|(from, to)| *from <= start && start < *to)
        {
            continue;
        }
        let trimmed = line.trim_end_matches(['\n', '\r']).trim_start();
        if let Some(directive) = parse_line(trimmed, author, sequence) {
            directives.push(directive);
        }
        if directives.len() == PIN_MARKER_CAP {
            break;
        }
    }
    directives
}

/// Fold a chronological slice of rows into a board of at most `limit` pins.
///
/// Later markers win: pinning an already-pinned message updates its label,
/// note and pinner rather than duplicating it, and an unpin removes it
/// whoever pinned it. When the board is over its limit the least recently
/// pinned entries are dropped — a full board is a signal that something has to
/// come off, and the oldest pin is the one the room stopped arguing about.
///
/// Pins are returned most recently pinned first.
#[must_use]
pub fn fold_pins(rows: &[LogMessage], limit: usize) -> Vec<Pin> {
    if limit == 0 {
        return Vec::new();
    }
    // A message carrying several markers gives them the same `pinned_at`
    // sequence, so directive reading order — not the sequence alone — has to
    // decide which one most recently touched the board. `ordinal` is that
    // order: it advances once per directive, in the row-then-marker order the
    // fold walks, and breaks the tie a sort on `pinned_at` alone could not.
    let mut board: BTreeMap<Sequence, (Pin, usize)> = BTreeMap::new();
    let mut ordinal = 0_usize;
    for row in rows {
        for directive in read_directives(&row.content, &row.author, row.sequence) {
            match directive.action {
                PinAction::Pin => {
                    board.insert(
                        directive.target,
                        (
                            Pin {
                                sequence: directive.target,
                                pinned_at: directive.sequence,
                                pinned_by: directive.author,
                                label: directive.label,
                                note: directive.note,
                                excerpt: None,
                            },
                            ordinal,
                        ),
                    );
                }
                PinAction::Unpin => {
                    board.remove(&directive.target);
                }
            }
            ordinal += 1;
        }
    }

    let excerpts: BTreeMap<Sequence, &str> = rows
        .iter()
        .map(|row| (row.sequence, row.content.as_str()))
        .collect();
    let mut pins: Vec<(Pin, usize)> = board
        .into_values()
        .map(|(mut pin, ordinal)| {
            pin.excerpt = excerpts
                .get(&pin.sequence)
                .map(|content| opening(content))
                .filter(|opening| !opening.is_empty());
            (pin, ordinal)
        })
        .collect();
    pins.sort_by_key(|(_, ordinal)| std::cmp::Reverse(*ordinal));
    pins.truncate(limit);
    pins.into_iter().map(|(pin, _)| pin).collect()
}

/// Read one conversation's board from the host log.
///
/// A desk-scoped read folds the desk's whole interior, thread replies
/// included: a pin exists to lift one message out of the depth it is buried
/// at, so refusing to look inside threads would defeat it. A thread-scoped
/// read folds that thread alone.
///
/// `before` is the same exclusive bound a caller passes as
/// [`crate::SessionQuery::before`]. Passing it through keeps the board and the
/// projected history reading the same snapshot, so a delayed or replayed turn
/// cannot see a pin — or an unpin — authored after its triggering message.
///
/// # Errors
///
/// Returns [`crate::Error::Read`] for a host read failure or a typed
/// page-validation error when a host violates ordering, uniqueness, size, or
/// cursor contracts.
pub async fn read_pinboard(
    log: &(dyn SessionLog + '_),
    conversation: &Conversation,
    limit: usize,
    before: Option<Sequence>,
) -> Result<Vec<Pin>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let rows = read_desk_rows(log, conversation, PIN_SCAN, before).await?;
    let rows: Vec<LogMessage> = match conversation.thread_root {
        None => rows,
        Some(_) => rows
            .into_iter()
            .filter(|row| matches_conversation(row, conversation))
            .collect(),
    };
    Ok(fold_pins(&rows, limit))
}

/// Render a board as one briefing note, or `None` when the board is empty.
#[must_use]
pub fn pin_note(pins: &[Pin]) -> Option<BriefingNote> {
    if pins.is_empty() {
        return None;
    }
    let lines = pins
        .iter()
        .map(|pin| {
            let mut line = format!("[{}]", pin.sequence);
            if let Some(label) = &pin.label {
                line.push_str(" #");
                line.push_str(label);
            }
            if let Some(excerpt) = &pin.excerpt {
                line.push_str(" \"");
                line.push_str(excerpt);
                line.push('"');
            }
            if let Some(note) = &pin.note {
                line.push_str(" — ");
                line.push_str(note);
            }
            line
        })
        .collect();
    Some(BriefingNote {
        heading: "Pinned in this conversation".to_owned(),
        lines,
    })
}

/// Parse one line as a pin marker.
fn parse_line(line: &str, author: &SessionAuthor, sequence: Sequence) -> Option<PinDirective> {
    let rest = line.strip_prefix('!')?;
    let mut words = rest.split_whitespace();
    let action = match words.next()? {
        "pin" => PinAction::Pin,
        "unpin" => PinAction::Unpin,
        _ => return None,
    };

    let mut target = None;
    let mut label = None;
    let mut note: Vec<&str> = Vec::new();
    for word in words {
        if let Some(value) = word.strip_prefix('^')
            && target.is_none()
            && note.is_empty()
            && let Ok(parsed) = value.parse()
        {
            target = Some(Sequence(parsed));
            continue;
        }
        if let Some(value) = word.strip_prefix('#')
            && label.is_none()
            && note.is_empty()
            && !value.is_empty()
        {
            label = Some(value.to_owned());
            continue;
        }
        note.push(word);
    }

    // An unpin with no target would remove the marker's own message, which is
    // never what anybody meant; it yields no directive at all, the same
    // fail-closed rule the trace grammar uses for an incomplete marker.
    let target = match (action, target) {
        (PinAction::Pin, target) => target.unwrap_or(sequence),
        (PinAction::Unpin, Some(target)) => target,
        (PinAction::Unpin, None) => return None,
    };

    Some(PinDirective {
        sequence,
        target,
        author: author.clone(),
        action,
        label,
        note: (!note.is_empty()).then(|| note.join(" ")),
    })
}

/// Take the opening words of a pinned message, cut on a character boundary.
fn opening(content: &str) -> String {
    let single_line: String = content.split_whitespace().collect::<Vec<_>>().join(" ");
    if single_line.chars().count() <= PIN_EXCERPT_CHARS {
        return single_line;
    }
    let mut opening: String = single_line.chars().take(PIN_EXCERPT_CHARS).collect();
    opening.push('…');
    opening
}

/// Byte ranges covered by fenced code blocks, which markers do not escape.
///
/// Follows the Markdown fence rule a marker's author would expect: a closing
/// fence must use the same character as the opener and be at least as long.
/// A shorter run of the same character — three backticks closing a
/// four-backtick block that itself contains an example fence — is content,
/// not a close, so tracking only the character and not its length would
/// resume directive parsing one line early and let quoted documentation
/// mutate the board.
fn fenced_ranges(body: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut open: Option<(usize, char, usize)> = None;
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let trimmed = line.trim_start();
        let fence = fence_run(trimmed, '`').or_else(|| fence_run(trimmed, '~'));
        let Some((char, len)) = fence else { continue };
        match open {
            None => open = Some((start, char, len)),
            Some((from, opener, opener_len)) if opener == char && len >= opener_len => {
                ranges.push((from, offset));
                open = None;
            }
            Some(_) => {}
        }
    }
    if let Some((from, ..)) = open {
        ranges.push((from, body.len()));
    }
    ranges
}

/// Whether a trimmed line opens or closes a fence built from `char`, and how
/// long the leading run of it is.
fn fence_run(trimmed: &str, char: char) -> Option<(char, usize)> {
    let len = trimmed.chars().take_while(|&c| c == char).count();
    (len >= 3).then_some((char, len))
}
