//! The stigmergic grammar: what a message deposits, and how it is read back.
//!
//! Coordination here is stigmergic in Grassé's sense — work leaves a trace in
//! a shared medium, and the trace is the stimulus for the next unit of work.
//! The transcript is the medium. No agent addresses another, and nothing in
//! this module dispatches anything.

#[cfg(test)]
mod test;

mod types;

pub use types::{Trace, TraceKind, TopicId};

use tinyteams::{SessionAuthor, SessionMessage, Sequence};

/// Maximum number of traces read from one message body.
///
/// A body that deposits more than this keeps the first `TRACE_CAP` in reading
/// order. The bound exists for the same reason [`MENTION_CAP`] does: one
/// message must not be able to grow the fold without limit.
///
/// [`MENTION_CAP`]: tinyteams_core::mention::MENTION_CAP
pub const TRACE_CAP: usize = 16;

/// Read traces from an authored body, or revalidate a supplied list.
///
/// `None` extracts from `body`; `Some`, including an empty vector, is
/// authoritative and is revalidated against the body. A body carrying no
/// marker yields no trace: ordinary conversation is never coerced into a vote.
///
/// Extraction recognises a marker only at the start of a line, ignoring
/// leading whitespace, and only outside a fenced code block. Inline backticks
/// need no masking, because a marker preceded by a backtick is by definition
/// not line-leading.
///
/// The grammar of one marker line is:
///
/// ```text
/// !<kind> [#topic] [>target] [^cite ...] [free text]
/// ```
#[must_use]
pub fn resolve(
    body: &str,
    supplied: Option<Vec<Trace>>,
    author: &SessionAuthor,
    sequence: Sequence,
) -> Vec<Trace> {
    let mut traces = match supplied {
        None => extract(body, author, sequence),
        Some(supplied) => revalidate(body, supplied, author, sequence),
    };
    traces.truncate(TRACE_CAP);
    traces
}

/// Fold a projected transcript into traces, in sequence order.
///
/// Messages carrying no marker contribute nothing, so a transcript of ordinary
/// conversation folds to an empty medium.
#[must_use]
pub fn read(messages: &[SessionMessage]) -> Vec<Trace> {
    let mut traces: Vec<Trace> = messages
        .iter()
        .flat_map(|message| resolve(&message.content, None, &message.author, message.sequence))
        .collect();
    traces.sort_by_key(|trace| (trace.sequence, trace.offset));
    traces
}

fn extract(body: &str, author: &SessionAuthor, sequence: Sequence) -> Vec<Trace> {
    let fenced = fenced_ranges(body);
    let mut traces = Vec::new();
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        if fenced.iter().any(|(from, to)| *from <= start && start < *to) {
            continue;
        }
        let trimmed = line.trim_end_matches(['\n', '\r']);
        let indent = trimmed.len() - trimmed.trim_start().len();
        let Some(trace) = parse_line(trimmed.trim_start(), author, sequence, start + indent) else {
            continue;
        };
        traces.push(trace);
    }
    traces
}

fn revalidate(
    body: &str,
    supplied: Vec<Trace>,
    author: &SessionAuthor,
    sequence: Sequence,
) -> Vec<Trace> {
    let extracted = extract(body, author, sequence);
    supplied
        .into_iter()
        .filter(|trace| {
            extracted
                .iter()
                .any(|found| found.offset == trace.offset && found.kind == trace.kind)
        })
        .map(|mut trace| {
            trace.author = author.clone();
            trace.sequence = sequence;
            trace
        })
        .collect()
}

fn parse_line(
    line: &str,
    author: &SessionAuthor,
    sequence: Sequence,
    offset: usize,
) -> Option<Trace> {
    let rest = line.strip_prefix('!')?;
    let mut words = rest.split_whitespace();
    let kind = match words.next()? {
        "propose" => TraceKind::Propose,
        "support" => TraceKind::Support,
        "object" => TraceKind::Object,
        "evidence" => TraceKind::Evidence,
        "question" => TraceKind::Question,
        "commit" => TraceKind::Commit,
        _ => return None,
    };

    let mut topic = None;
    let mut target = None;
    let mut cites = Vec::new();
    for word in words {
        if let Some(value) = word.strip_prefix('#') {
            if topic.is_none() && !value.is_empty() {
                topic = Some(TopicId(value.to_owned()));
            }
        } else if let Some(value) = word.strip_prefix('>') {
            if target.is_none() {
                target = value.parse().ok().map(Sequence);
            }
        } else if let Some(value) = word.strip_prefix('^')
            && let Ok(parsed) = value.parse()
        {
            let cited = Sequence(parsed);
            if !cites.contains(&cited) {
                cites.push(cited);
            }
        }
    }

    Some(Trace {
        sequence,
        author: author.clone(),
        kind,
        topic,
        target,
        cites,
        text: line.to_owned(),
        offset,
    })
}

fn fenced_ranges(body: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut open: Option<usize> = None;
    let mut offset = 0;
    for line in body.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        if !line.trim_start().starts_with("```") {
            continue;
        }
        match open.take() {
            None => open = Some(start),
            Some(from) => ranges.push((from, offset)),
        }
    }
    if let Some(from) = open {
        ranges.push((from, body.len()));
    }
    ranges
}
