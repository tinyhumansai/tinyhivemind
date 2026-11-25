//! Which harness a seat runs on.
use serde::{Deserialize, Serialize};
use std::fmt;

/// A harness, by the name a package gives it.
///
/// Only a name: what a kind can do is the language's to check, and the
/// program that does it is the host's to choose.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HarnessKind {
    /// `openhuman`.
    OpenHuman,
    /// `openclaw`.
    OpenClaw,
    /// `codex`.
    Codex,
}

impl fmt::Display for HarnessKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OpenHuman => "openhuman",
            Self::OpenClaw => "openclaw",
            Self::Codex => "codex",
        })
    }
}

#[cfg(test)]
mod test;
