//! The euler-hive package as the host loads it: read from disk, the run's
//! problem injected, parsed and lowered, and each seat's tool belt resolved.
//!
//! The committed package carries a placeholder `context/problem.md`. The host
//! replaces it with the statement it was handed, converted to Markdown, so the
//! repository never holds a problem's data and the same package serves any
//! problem.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use tinyhivemind_core::desk::Desk;
use tinyhivemind_lang::{LoweredHive, LoweredSeat, Package, lower, parse};

/// The context document the host replaces at run time.
pub(super) const PROBLEM_DOCUMENT: &str = "context/problem.md";

/// One tool the host can put on a seat's belt.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum SeatTool {
    /// Search the public web through the TinyHumans backend.
    WebSearch,
    /// Run Python in the network-less Docker sandbox.
    RunPython,
    /// Hand work to a colleague on the seat's own desk.
    Broadcast,
    /// Refer work to another desk, through core's referral fold.
    Refer,
    /// Finish the seat's assignment with a report.
    Complete,
    /// Write to the seat's bound memory (or the verified pool).
    Remember,
    /// Read the seat's bound memory reaches.
    Recall,
}

impl SeatTool {
    /// Every tool the host serves.
    pub(super) const ALL: [Self; 7] = [
        Self::WebSearch,
        Self::RunPython,
        Self::Broadcast,
        Self::Refer,
        Self::Complete,
        Self::Remember,
        Self::Recall,
    ];

    /// The tool's name, as a package's `tools.allow` spells it.
    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::WebSearch => "web_search",
            Self::RunPython => "run_python",
            Self::Broadcast => "hive_broadcast",
            Self::Refer => "hive_refer",
            Self::Complete => "hive_complete",
            Self::Remember => "hive_remember",
            Self::Recall => "hive_recall",
        }
    }

    fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tool| tool.name() == name)
    }
}

/// Resolve a seat's belt from its lowered `tools`, refusing what the host
/// cannot honour.
///
/// The allow list is the whole belt: a name the host does not serve is an
/// error rather than silently dropped, and a name both allowed and denied is
/// refused. The harness's own tools (shell, files, fetch) never appear: the
/// host builds every seat host-only.
///
/// # Errors
///
/// Fails on an unknown allowed tool or an allow/deny conflict.
pub(super) fn seat_tools(seat: &LoweredSeat) -> anyhow::Result<BTreeSet<SeatTool>> {
    let mut belt = BTreeSet::new();
    for name in &seat.integration.tools.allow {
        if seat.integration.tools.deny.contains(name) {
            anyhow::bail!("{} both allows and denies {name}", seat.id);
        }
        let tool = SeatTool::named(name).ok_or_else(|| {
            anyhow::anyhow!("{} allows {name}, which this host does not serve", seat.id)
        })?;
        belt.insert(tool);
    }
    for required in [SeatTool::Broadcast, SeatTool::Refer, SeatTool::Complete] {
        if !belt.contains(&required) {
            anyhow::bail!("{} cannot end a turn without {}", seat.id, required.name());
        }
    }
    Ok(belt)
}

/// Read a package directory into the document map `parse` takes.
///
/// # Errors
///
/// Fails when `dir` is unreadable, holds a non-UTF-8 file, or holds a
/// symbolic link.
pub(super) fn read_package(dir: &Path) -> anyhow::Result<BTreeMap<String, String>> {
    let mut documents = BTreeMap::new();
    walk(dir, dir, &mut documents)?;
    if !documents.contains_key("hive.json") {
        anyhow::bail!("{} has no hive.json", dir.display());
    }
    Ok(documents)
}

fn walk(root: &Path, dir: &Path, into: &mut BTreeMap<String, String>) -> anyhow::Result<()> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<_>>()?;
    entries.sort();
    for path in entries {
        // A link could point anywhere on disk, and the package's files become
        // model prompts: refuse rather than follow it.
        if std::fs::symlink_metadata(&path)?.file_type().is_symlink() {
            anyhow::bail!(
                "{} is a symbolic link; package files must be regular files",
                path.display()
            );
        }
        if path.is_dir() {
            walk(root, &path, into)?;
            continue;
        }
        let key: Vec<String> = path
            .strip_prefix(root)?
            .components()
            .map(|part| part.as_os_str().to_string_lossy().into_owned())
            .collect();
        into.insert(key.join("/"), std::fs::read_to_string(&path)?);
    }
    Ok(())
}

/// Load, inject `problem` (Markdown) as the problem document, then parse and
/// lower.
///
/// # Errors
///
/// Fails when the package has no problem document to replace, or when parsing
/// or lowering rejects it.
pub(super) fn load(
    mut documents: BTreeMap<String, String>,
    problem: &str,
) -> anyhow::Result<(Package, LoweredHive)> {
    let slot = documents
        .get_mut(PROBLEM_DOCUMENT)
        .ok_or_else(|| anyhow::anyhow!("the package declares no {PROBLEM_DOCUMENT}"))?;
    *slot = format!("# Problem\n\n<!-- item:statement -->\n{}\n", problem.trim());
    let package = parse(&documents)?;
    let lowered = lower(&package)?;
    if !lowered.context.contains_key(PROBLEM_DOCUMENT) {
        anyhow::bail!("the package does not list {PROBLEM_DOCUMENT} as context");
    }
    Ok((package, lowered))
}

/// The desk a seat sits on.
pub(super) fn desk_of<'a>(hive: &'a LoweredHive, seat: &str) -> Option<&'a Desk> {
    hive.desks
        .iter()
        .find(|desk| desk.members.iter().any(|member| member == seat))
}

/// Convert a minimal problem page to Markdown: paragraphs kept, tags dropped,
/// entities decoded, superscripts/subscripts made explicit, TeX left as written.
pub(super) fn problem_markdown(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        text.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>') else {
            rest = "";
            break;
        };
        let raw = rest[start + 1..start + end].trim();
        let closing = raw.starts_with('/');
        let tag = raw.trim_start_matches('/').split_whitespace().next()
            .unwrap_or("").trim_end_matches('/').to_ascii_lowercase();
        match tag.as_str() {
            "sup" => text.push_str(if closing { ")" } else { "^(" }),
            "sub" => text.push_str(if closing { ")" } else { "_(" }),
            "p" | "br" | "div" => text.push_str("\n\n"),
            _ => {}
        }
        rest = &rest[start + end + 1..];
    }
    text.push_str(rest);
    let decoded = text
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    let mut paragraphs = Vec::new();
    for block in decoded.split("\n\n") {
        let lines: Vec<&str> = block
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        if !lines.is_empty() {
            paragraphs.push(lines.join("\n"));
        }
    }
    paragraphs.join("\n\n")
}
