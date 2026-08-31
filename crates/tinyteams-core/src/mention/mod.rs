//! Mention extraction, revalidation, normalization, and pure routing choices.

#[cfg(test)]
mod test;

mod types;

pub use types::{Mention, MentionAuthor, MentionTarget};

use crate::{chat::is_general_chat, desk::DeskSet, roster::Roster};

/// Maximum number of mentions in one message that may remain pinging.
pub const MENTION_CAP: usize = 50;

#[derive(Clone, Debug)]
struct Alias {
    text: String,
    target: MentionTarget,
    desk: bool,
}

/// Resolve extracted or host-supplied mentions against current snapshots.
///
/// `None` extracts from `body`; `Some`, including an empty vector, is
/// authoritative. Invalid structures fail closed as an empty result. Supplied
/// stale references remain visible but are made quiet.
#[must_use]
pub fn resolve(
    body: &str,
    supplied: Option<Vec<Mention>>,
    author: &MentionAuthor,
    roster: &Roster<'_>,
    desks: &DeskSet<'_>,
) -> Vec<Mention> {
    if roster.validate().is_err() || desks.validate().is_err() {
        return Vec::new();
    }
    let aliases = aliases(roster, desks);
    let masked = code_ranges(body);
    let mentions = match supplied {
        None => extract(body, &aliases, &masked),
        Some(mentions) => revalidate(body, mentions, &aliases, &masked, roster, desks),
    };
    normalize(mentions, author)
}

/// Select the first nonquiet active agent mention in reading order.
///
/// Person, desk, and everyone mentions deliberately cannot start an agent
/// turn. This function only selects an id and never dispatches.
#[must_use]
pub fn direct_responder<'a>(mentions: &[Mention], roster: &'a Roster<'a>) -> Option<&'a str> {
    if roster.validate().is_err() {
        return None;
    }
    let mut ordered: Vec<&Mention> = mentions.iter().collect();
    ordered.sort_by_key(|mention| mention.offset);
    ordered.into_iter().find_map(|mention| {
        if mention.quiet {
            return None;
        }
        let MentionTarget::Agent { id } = &mention.target else {
            return None;
        };
        roster.active_member(id).map(|member| member.id.as_str())
    })
}

/// Expand mention targets into active agent ids needed as turn context.
///
/// Expansion preserves mention and desk/roster order, deduplicates the first
/// appearance, and excludes `responder`. It never dispatches any member.
#[must_use]
pub fn mentioned_members(
    mentions: &[Mention],
    addressed_desk: Option<&str>,
    responder: Option<&str>,
    roster: &Roster<'_>,
    desks: &DeskSet<'_>,
) -> Vec<String> {
    if roster.validate().is_err() || desks.validate().is_err() {
        return Vec::new();
    }
    let mut ordered: Vec<&Mention> = mentions.iter().collect();
    ordered.sort_by_key(|mention| mention.offset);
    let mut result = Vec::new();
    for mention in ordered {
        match &mention.target {
            MentionTarget::Agent { id } => push_member(&mut result, id, responder, roster),
            MentionTarget::Desk { id } => {
                if let Ok(members) = desks.members(id) {
                    for id in members {
                        push_member(&mut result, id, responder, roster);
                    }
                }
            }
            MentionTarget::Everyone => match addressed_desk {
                Some(desk) if !is_general_chat(Some(desk)) => {
                    if let Ok(members) = desks.members(desk) {
                        for id in members {
                            push_member(&mut result, id, responder, roster);
                        }
                    }
                }
                _ => {
                    for member in roster.active_members() {
                        push_once(&mut result, &member.id, responder);
                    }
                }
            },
            MentionTarget::Person { .. } => {}
        }
    }
    result
}

fn push_member(result: &mut Vec<String>, id: &str, responder: Option<&str>, roster: &Roster<'_>) {
    if let Some(member) = roster.active_member(id) {
        push_once(result, &member.id, responder);
    }
}

fn push_once(result: &mut Vec<String>, id: &str, responder: Option<&str>) {
    if responder != Some(id) && !result.iter().any(|existing| existing == id) {
        result.push(id.to_owned());
    }
}

fn aliases(roster: &Roster<'_>, desks: &DeskSet<'_>) -> Vec<Alias> {
    let mut result = Vec::new();
    for member in roster.active_members() {
        add_alias(
            &mut result,
            &member.id,
            MentionTarget::Agent {
                id: member.id.clone(),
            },
            false,
        );
        if let Some(name) = member
            .name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
        {
            add_alias(
                &mut result,
                name,
                MentionTarget::Agent {
                    id: member.id.clone(),
                },
                false,
            );
        }
    }

    let people: Vec<_> = roster.people().collect();
    let mut allocated_slugs = Vec::new();
    for person in people {
        let target = MentionTarget::Person {
            id: person.id.clone(),
        };
        if !person.label.trim().is_empty() {
            add_alias(&mut result, &person.label, target.clone(), false);
        }
        let base = ascii_slug(&person.label);
        if !base.is_empty() {
            let mut slug = base.clone();
            let mut suffix = 2;
            while allocated_slugs.contains(&slug) {
                slug = format!("{base}_{suffix}");
                suffix += 1;
            }
            allocated_slugs.push(slug.clone());
            add_alias(&mut result, &slug, target, false);
        }
    }

    for desk in desks.iter() {
        let target = MentionTarget::Desk {
            id: desk.id.clone(),
        };
        add_alias(&mut result, &desk.id, target.clone(), true);
        add_alias(&mut result, &desk.name, target, true);
    }
    for alias in ["everyone", "channel", "here"] {
        add_alias(&mut result, alias, MentionTarget::Everyone, false);
    }
    result.sort_by_key(|alias| std::cmp::Reverse(alias.text.len()));
    result
}

fn add_alias(result: &mut Vec<Alias>, text: &str, target: MentionTarget, desk: bool) {
    if text.chars().next().is_some_and(valid_alias_start)
        && text
            .chars()
            .skip(1)
            .all(|character| !closes_alias(character))
    {
        result.push(Alias {
            text: text.to_owned(),
            target,
            desk,
        });
    }
}

fn ascii_slug(label: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for character in label.chars() {
        if character.is_ascii_alphanumeric() {
            if separator && !slug.is_empty() {
                slug.push('_');
            }
            separator = false;
            slug.push(character.to_ascii_lowercase());
        } else {
            separator = true;
        }
    }
    slug
}

fn extract(body: &str, aliases: &[Alias], masked: &[(usize, usize)]) -> Vec<Mention> {
    body.char_indices()
        .filter(|(offset, character)| {
            *character == '@' && opens_at(body, *offset) && !is_masked(*offset, masked)
        })
        .filter_map(|(offset, _)| extract_at(body, offset, aliases))
        .collect()
}

fn extract_at(body: &str, offset: usize, aliases: &[Alias]) -> Option<Mention> {
    let (hash, authored, end) = mention_token(body, offset)?;

    let matches: Vec<&Alias> = aliases
        .iter()
        .filter(|alias| !hash || alias.desk)
        .filter(|alias| same_alias(authored, &alias.text))
        .collect();
    let first_target = &matches.first()?.target;
    if matches.iter().any(|alias| alias.target != *first_target) {
        return None;
    }
    Some(Mention {
        target: first_target.clone(),
        text: body[offset..end].to_owned(),
        offset,
        quiet: false,
    })
}

fn revalidate(
    body: &str,
    supplied: Vec<Mention>,
    aliases: &[Alias],
    masked: &[(usize, usize)],
    roster: &Roster<'_>,
    desks: &DeskSet<'_>,
) -> Vec<Mention> {
    supplied
        .into_iter()
        .filter_map(|mut mention| {
            let end = mention.offset.checked_add(mention.text.len())?;
            if !body.is_char_boundary(mention.offset)
                || !body.is_char_boundary(end)
                || body.get(mention.offset..end)? != mention.text
                || is_masked(mention.offset, masked)
                || !mention_shaped(body, &mention, end)
            {
                return None;
            }
            let current = extract_at(body, mention.offset, aliases).map(|found| found.target);
            if current.as_ref() != Some(&mention.target)
                || !target_is_active(&mention.target, roster, desks)
            {
                mention.quiet = true;
            }
            Some(mention)
        })
        .collect()
}

fn mention_shaped(body: &str, mention: &Mention, end: usize) -> bool {
    opens_at(body, mention.offset)
        && mention_token(body, mention.offset)
            .is_some_and(|(_, _, extracted_end)| extracted_end == end)
}

fn mention_token(body: &str, offset: usize) -> Option<(bool, &str, usize)> {
    if body.as_bytes().get(offset) != Some(&b'@') {
        return None;
    }
    let hash = body.as_bytes().get(offset + 1) == Some(&b'#');
    let start = offset + if hash { 2 } else { 1 };
    let rest = body.get(start..)?;
    let mut characters = rest.char_indices();
    let (_, first) = characters.next()?;
    if !valid_alias_start(first) {
        return None;
    }
    let relative_end = characters
        .find_map(|(index, character)| closes_alias(character).then_some(index))
        .unwrap_or(rest.len());
    let authored = rest.get(..relative_end)?;
    if authored.chars().any(|character| character == '@') {
        return None;
    }
    Some((hash, authored, start + relative_end))
}

fn target_is_active(target: &MentionTarget, roster: &Roster<'_>, desks: &DeskSet<'_>) -> bool {
    match target {
        MentionTarget::Agent { id } => roster.active_member(id).is_some(),
        MentionTarget::Person { id } => roster.person(id).is_some(),
        MentionTarget::Desk { id } => desks.resolve_id(id).is_ok(),
        MentionTarget::Everyone => true,
    }
}

fn normalize(mut mentions: Vec<Mention>, author: &MentionAuthor) -> Vec<Mention> {
    mentions.sort_by_key(|mention| mention.offset);
    let mut offsets = Vec::new();
    let mut targets = Vec::new();
    let mut pinging = 0;
    mentions.retain_mut(|mention| {
        if offsets.contains(&mention.offset) || is_self(&mention.target, author) {
            return false;
        }
        offsets.push(mention.offset);
        if targets.contains(&mention.target) {
            mention.quiet = true;
        } else {
            targets.push(mention.target.clone());
        }
        if !mention.quiet {
            if pinging == MENTION_CAP {
                mention.quiet = true;
            } else {
                pinging += 1;
            }
        }
        true
    });
    mentions
}

fn is_self(target: &MentionTarget, author: &MentionAuthor) -> bool {
    match (target, author) {
        (MentionTarget::Agent { id: target }, MentionAuthor::Agent { id: author })
        | (MentionTarget::Person { id: target }, MentionAuthor::Person { id: author }) => {
            target == author
        }
        _ => false,
    }
}

fn same_alias(left: &str, right: &str) -> bool {
    left.to_lowercase() == right.to_lowercase()
}

fn valid_alias_start(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn opens_at(body: &str, offset: usize) -> bool {
    offset == 0
        || body
            .as_bytes()
            .get(offset.wrapping_sub(1))
            .is_some_and(|byte| byte.is_ascii_whitespace() || matches!(*byte, b'(' | b'[' | b'{'))
}

fn closes_alias(character: char) -> bool {
    character.is_whitespace() || ",;.?!:)]}'\"".contains(character)
}

fn is_masked(offset: usize, ranges: &[(usize, usize)]) -> bool {
    ranges
        .iter()
        .any(|(start, end)| *start <= offset && offset < *end)
}

fn code_ranges(body: &str) -> Vec<(usize, usize)> {
    let mut ranges = fenced_ranges(body);
    ranges.extend(inline_ranges(body, &ranges));
    ranges.sort_unstable();
    ranges
}

fn inline_ranges(body: &str, fenced: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let bytes = body.as_bytes();
    let mut ranges = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        if let Some((_, end)) = fenced
            .iter()
            .find(|(start, end)| *start <= offset && offset < *end)
        {
            offset = *end;
            continue;
        }
        if bytes[offset] != b'`' {
            offset += 1;
            continue;
        }
        let run = bytes[offset..]
            .iter()
            .take_while(|byte| **byte == b'`')
            .count();
        let mut candidate = offset + run;
        let mut closing = None;
        while candidate < bytes.len() {
            if is_masked(candidate, fenced) || bytes[candidate] != b'`' {
                candidate += 1;
                continue;
            }
            let closing_run = bytes[candidate..]
                .iter()
                .take_while(|byte| **byte == b'`')
                .count();
            if closing_run == run {
                closing = Some(candidate + closing_run);
                break;
            }
            candidate += closing_run;
        }
        if let Some(end) = closing {
            ranges.push((offset, end));
            offset = end;
        } else {
            offset += run;
        }
    }
    ranges
}

fn fenced_ranges(body: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut open: Option<(usize, u8, usize)> = None;
    let mut line_start = 0;
    for line in body.split_inclusive('\n') {
        let trimmed = line.trim_start_matches(' ');
        let indent = line.len() - trimmed.len();
        if indent <= 3 {
            let marker = trimmed.as_bytes().first().copied();
            if matches!(marker, Some(b'`' | b'~')) {
                let marker = marker.unwrap_or_default();
                let run = trimmed
                    .as_bytes()
                    .iter()
                    .take_while(|byte| **byte == marker)
                    .count();
                if run >= 3 {
                    match open {
                        None if marker == b'~' || !trimmed[run..].contains('`') => {
                            open = Some((line_start, marker, run));
                        }
                        Some((start, open_marker, open_run))
                            if marker == open_marker
                                && run >= open_run
                                && trimmed[run..].trim().is_empty() =>
                        {
                            ranges.push((start, line_start + line.len()));
                            open = None;
                        }
                        None | Some(_) => {}
                    }
                }
            }
        }
        line_start += line.len();
    }
    if let Some((start, _, _)) = open {
        ranges.push((start, body.len()));
    }
    ranges
}
