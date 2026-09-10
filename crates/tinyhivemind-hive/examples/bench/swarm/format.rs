//! The wire text a swarm's members exchange, and the trace line a run keeps.
//!
//! A referral carries a plain line of text between desks, exactly as a real
//! agent's reply would. [`readings`] and [`restate`] are the two halves of
//! that convention: `readings` reads a desk's numeric view of every option out
//! of a line written the way a member would write it, and `restate` writes a
//! set of readings back out in the same form, so a reply that carries them on
//! can be read the same way at the far end. [`line`] is unrelated to the wire
//! format itself — it renders one transcript entry for the `--trace` view.
//!
//! [`facts`] reads the *other* kind of thing a desk can say. A reading is an
//! opinion and averages with its reader's own; a fact is a disqualification
//! and does not. They travel in the same row and are parsed independently —
//! `<Desk> reads #a at 40, #b at 100. <Desk> rules out #a.` — so a line
//! carrying both is read the same way by a reader that understands only the
//! first half.

use std::fmt::Write as _;

use tinyhivemind_hive::{Sequence, trace::TopicId};

use super::Channel;

/// One desk's reading of one option, as it appears in a message.
#[derive(Clone, Debug)]
pub(super) struct Reading {
    /// The desk that holds the reading, by display name.
    pub(super) desk: String,
    /// The option it is a reading of.
    pub(super) topic: TopicId,
    /// What that desk rates it.
    pub(super) value: i32,
}

/// Write a set of readings back out in the form they were read from.
pub(super) fn restate(readings: &[Reading]) -> String {
    let mut line = String::new();
    let mut current: Option<&str> = None;
    for reading in readings {
        if current.is_none_or(|desk| desk != reading.desk) {
            if current.is_some() {
                line.push_str(". ");
            }
            let _ = write!(line, "{} reads", reading.desk);
            current = Some(reading.desk.as_str());
        } else {
            line.push(',');
        }
        let _ = write!(line, " #{} at {}", reading.topic, reading.value);
    }
    if current.is_some() {
        line.push('.');
    }
    line
}

/// Read every `<Desk> reads #option at <n>, #option at <n>.` clause out of a
/// message.
///
/// The form is the one the members write, and it is meant to survive a real
/// agent writing it in a sentence of its own rather than as a field: a desk
/// name followed by `reads` opens a clause, and every `#option at <n>` until
/// the next desk name belongs to it.
pub(super) fn readings(content: &str) -> Vec<Reading> {
    let words: Vec<&str> = content.split_whitespace().collect();
    let mut found = Vec::new();
    let mut desk: Option<String> = None;
    for (at, word) in words.iter().enumerate() {
        if *word == "reads"
            && let Some(name) = at.checked_sub(1).and_then(|before| words.get(before))
        {
            desk = Some((*name).to_owned());
            continue;
        }
        let Some(topic) = word.strip_prefix('#') else {
            continue;
        };
        let topic = topic.trim_end_matches(['.', ',', ';', '?', '!']);
        if topic.is_empty() || words.get(at + 1) != Some(&"at") {
            continue;
        }
        let Some(value) = words.get(at + 2) else {
            continue;
        };
        let Ok(value) = value.trim_end_matches(['.', ',', ';']).parse::<i32>() else {
            continue;
        };
        let Some(desk) = desk.clone() else {
            continue;
        };
        found.push(Reading {
            desk,
            topic: TopicId::from(topic),
            value,
        });
    }
    found
}

/// One desk's disqualification of one option, as it appears in a message.
///
/// Deliberately not a [`Reading`] with a sentinel value. A reading is averaged
/// into the reader's own and a fact is subtracted from it whatever anyone else
/// thinks, so the two are different operations on the far end and giving them
/// one type would invite a caller to treat them as one.
#[derive(Clone, Debug)]
pub(super) struct Fact {
    /// The desk that holds it, by display name.
    pub(super) desk: String,
    /// The option it disqualifies.
    pub(super) topic: TopicId,
}

/// Write a set of facts out in the form [`facts`] reads them from.
pub(super) fn restate_facts(held: &[Fact]) -> String {
    let mut line = String::new();
    let mut current: Option<&str> = None;
    for fact in held {
        if current.is_none_or(|desk| desk != fact.desk) {
            if current.is_some() {
                line.push_str(". ");
            }
            let _ = write!(line, "{} rules out", fact.desk);
            current = Some(fact.desk.as_str());
        } else {
            line.push(',');
        }
        let _ = write!(line, " #{}", fact.topic);
    }
    if current.is_some() {
        line.push('.');
    }
    line
}

/// Read every `<Desk> rules out #option.` clause out of a message.
///
/// Shaped exactly like [`readings`] and parsed independently of it: a desk
/// name followed by `rules out` opens a clause, and every `#option` until the
/// next desk name belongs to it. An option that carries an `at <n>` is a
/// reading rather than a disqualification and is left to [`readings`], so the
/// two can share a row without either having to know about the other.
pub(super) fn facts(content: &str) -> Vec<Fact> {
    let words: Vec<&str> = content.split_whitespace().collect();
    let mut found = Vec::new();
    let mut desk: Option<String> = None;
    for (at, word) in words.iter().enumerate() {
        if *word == "rules" && words.get(at + 1) == Some(&"out") {
            desk = at
                .checked_sub(1)
                .and_then(|before| words.get(before))
                .map(|name| (*name).to_owned());
            continue;
        }
        // A desk name followed by `reads` closes any open fact clause: what
        // follows is that desk's opinion, not its disqualifications.
        if *word == "reads" {
            desk = None;
            continue;
        }
        let Some(topic) = word.strip_prefix('#') else {
            continue;
        };
        let topic = topic.trim_end_matches(['.', ',', ';', '?', '!']);
        if topic.is_empty() || words.get(at + 1) == Some(&"at") {
            continue;
        }
        let Some(desk) = desk.clone() else {
            continue;
        };
        found.push(Fact {
            desk,
            topic: TopicId::from(topic),
        });
    }
    found
}

/// One transcript line, tagged with the channel it was written in.
pub(super) fn line(
    channels: &[Channel],
    desk: usize,
    sequence: Sequence,
    agent_id: &str,
    content: &str,
) -> String {
    format!(
        "{:>9} {:>3}  {:<18} {content}",
        channels[desk].name, sequence.0, agent_id,
    )
}
