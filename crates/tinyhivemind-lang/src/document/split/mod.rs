//! The manifest split across files: `hive.json` plus the optional
//! `agents.json`, `tools.json` and `desks.json`.
//!
//! Each split file is a JSON object holding some of the manifest's top-level
//! sections under their usual keys, and only the sections that file owns
//! ([`SPLITS`]). Parsing assembles them into the one [`super::Manifest`]
//! before decoding, so a split package decodes, validates, lowers and
//! patches exactly like the same package written as one `hive.json`. A
//! section lives in one file only: declared in both `hive.json` and its split
//! file, it is reported in the split file. Every finding about a section
//! that came from a split file is reported in that file, at the same JSON
//! Pointer, so the split is invisible everywhere but in locations.
use super::{Profile, Role, Seat, decode};
use crate::{
    Error,
    diagnostic::{Findings, HIVE, escape},
};
use crate::{ExecTool, Hook, McpServer, Skill, Toolpack};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use tinyhivemind_core::desk::Desk;

/// `agents.json`: who the hive's agents are.
pub(crate) const AGENTS: &str = "agents.json";
/// `tools.json`: what the agents can run.
pub(crate) const TOOLS: &str = "tools.json";
/// `desks.json`: how the agents are organized.
pub(crate) const DESKS: &str = "desks.json";

/// Each split file and the manifest sections it may hold.
pub(crate) const SPLITS: [(&str, &[&str]); 3] = [
    (AGENTS, &["roles", "seats", "profiles"]),
    (TOOLS, &["tools", "toolpacks", "mcp", "skills", "hooks"]),
    (DESKS, &["desks", "topology", "referrals"]),
];

/// The typed form of `agents.json`, decoded on its own so a schema error is
/// located by line and column in that file.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AgentsFile {
    #[serde(default)]
    roles: Vec<Role>,
    #[serde(default)]
    seats: Vec<Seat>,
    #[serde(default)]
    profiles: Vec<Profile>,
}

/// The typed form of `tools.json`.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ToolsFile {
    #[serde(default)]
    tools: Vec<ExecTool>,
    #[serde(default)]
    toolpacks: Vec<Toolpack>,
    #[serde(default)]
    mcp: Vec<McpServer>,
    #[serde(default)]
    skills: Vec<Skill>,
    #[serde(default)]
    hooks: Vec<Hook>,
}

/// The typed form of `desks.json`.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DesksFile {
    #[serde(default, deserialize_with = "super::defaults::desks")]
    desks: Vec<Desk>,
    #[serde(default)]
    topology: Vec<super::Link>,
    #[serde(default)]
    referrals: Vec<super::Link>,
}

/// Decode the manifest from `hive.json` and whichever split files are
/// present, recording every finding.
pub(super) fn manifest(
    documents: &BTreeMap<String, String>,
    found: &mut Findings,
) -> Option<super::Manifest> {
    let present: Vec<_> = SPLITS
        .iter()
        .filter(|(file, _)| documents.contains_key(*file))
        .collect();
    if present.is_empty() {
        return decode(documents, HIVE, found);
    }
    let mut parts = Some(Vec::new());
    for (file, _) in &present {
        let part = match *file {
            AGENTS => decode::<AgentsFile>(documents, file, found).map(|_| ()),
            TOOLS => decode::<ToolsFile>(documents, file, found).map(|_| ()),
            _ => decode::<DesksFile>(documents, file, found).map(|_| ()),
        }
        .and_then(|()| serde_json::from_str::<Map<String, Value>>(&documents[*file]).ok());
        match (part, parts.as_mut()) {
            (Some(part), Some(parts)) => parts.push((*file, part)),
            _ => parts = None,
        }
    }
    let Some(source) = documents.get(HIVE) else {
        found.push(Error::MissingDocument { path: HIVE.into() }, HIVE, None);
        return None;
    };
    let mut hive = match serde_json::from_str::<Value>(source) {
        Ok(Value::Object(hive)) => hive,
        // Not an object: decoding `hive.json` alone reports it, located.
        _ => return decode(documents, HIVE, found),
    };
    let mut assembled = true;
    for (file, part) in parts? {
        for (section, value) in part {
            if hive.contains_key(&section) {
                let message = format!("section {section} is also declared in {HIVE}");
                let pointer = format!("/{}", escape(&section));
                found.push(json(file, message), file, pointer);
                assembled = false;
            } else {
                hive.insert(section, value);
            }
        }
    }
    if !assembled {
        return None;
    }
    super::decode_value(HIVE, None, Value::Object(hive), found)
}

fn json(path: &str, message: String) -> Error {
    Error::Json {
        path: path.into(),
        message,
    }
}

/// The file a manifest finding belongs in: the split file that holds the
/// section its pointer starts in, when `documents` has that file, and
/// `file` otherwise.
pub(crate) fn home<'a>(
    file: &'a str,
    pointer: Option<&str>,
    documents: &BTreeMap<String, String>,
) -> &'a str {
    if file != HIVE {
        return file;
    }
    let Some(section) = pointer.and_then(|pointer| pointer.split('/').nth(1)) else {
        return file;
    };
    SPLITS
        .iter()
        .find(|(split, sections)| sections.contains(&section) && documents.contains_key(*split))
        .map_or(file, |(split, _)| split)
}

#[cfg(test)]
mod test;
