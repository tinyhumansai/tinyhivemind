//! Hook matchers: tool names with `*` wildcards.

/// Whether `tool` is one `matcher` names. `None` names every tool; `*`
/// matches any run of characters, including none.
pub(crate) fn matches(matcher: Option<&str>, tool: &str) -> bool {
    let Some(pattern) = matcher else {
        return true;
    };
    let mut pieces = pattern.split('*');
    let first = pieces.next().unwrap_or_default();
    let Some(mut rest) = tool.strip_prefix(first) else {
        return false;
    };
    let mut pieces: Vec<&str> = pieces.collect();
    let Some(last) = pieces.pop() else {
        // No `*`: the pattern is the whole name.
        return rest.is_empty();
    };
    for piece in pieces {
        match rest.find(piece) {
            Some(at) => rest = &rest[at + piece.len()..],
            None => return false,
        }
    }
    rest.ends_with(last)
}
