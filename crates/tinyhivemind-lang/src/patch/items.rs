//! Stable item manipulation without rewriting neighboring Markdown.

use super::{Error, Result};

pub(super) fn body_offset(document: &str) -> usize {
    if let Some(tail) = document.strip_prefix("---\n")
        && let Some(end) = tail.find("\n---\n")
    {
        return 4 + end + 5;
    }
    0
}

pub(super) fn items(document: &str) -> Result<Vec<(String, usize, usize, String)>> {
    let mut starts = Vec::new();
    let mut offset = 0;
    for line in document.split_inclusive('\n') {
        let trimmed = line.trim_end_matches('\n');
        if let Some(id) = trimmed.strip_prefix("<!-- item:") {
            let id = id
                .strip_suffix(" -->")
                .ok_or_else(|| Error::Item(trimmed.into()))?;
            if id.is_empty()
                || id
                    .chars()
                    .any(|c| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
                || starts.iter().any(|(previous, _, _)| previous == id)
            {
                return Err(Error::Item(id.into()));
            }
            starts.push((id.to_owned(), offset, offset + line.len()));
        }
        offset += line.len();
    }
    Ok(starts
        .iter()
        .enumerate()
        .map(|(index, (id, start, body))| {
            let end = starts
                .get(index + 1)
                .map_or(document.len(), |(_, start, _)| *start);
            (id.clone(), *start, end, document[*body..end].to_owned())
        })
        .collect())
}

pub(super) fn edit(
    document: &str,
    id: &str,
    before: Option<&str>,
    after: Option<&str>,
) -> Result<String> {
    if id.is_empty()
        || id
            .chars()
            .any(|c| !c.is_ascii_alphanumeric() && c != '-' && c != '_')
        || after.is_some_and(|body| body.lines().any(|line| line.starts_with("<!-- item:")))
    {
        return Err(Error::Item(id.into()));
    }
    let entries = items(document)?;
    let existing = entries.iter().find(|(key, _, _, _)| key == id);
    if existing.map(|(_, _, _, body)| body.as_str()) != before {
        return Err(Error::Stale(id.into()));
    }
    let replacement = after.map_or_else(String::new, |body| format!("<!-- item:{id} -->\n{body}"));
    if after.is_some_and(|body| !body.ends_with('\n')) {
        return Err(Error::Item(id.into()));
    }
    let mut result = document.to_owned();
    if let Some((_, start, end, _)) = existing {
        result.replace_range(*start..*end, &replacement);
    } else if !replacement.is_empty() {
        if !result.is_empty() && !result.ends_with('\n') {
            return Err(Error::Item(id.into()));
        }
        result.push_str(&replacement);
    }
    Ok(result)
}
