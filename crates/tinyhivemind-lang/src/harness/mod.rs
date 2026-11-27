//! Which harness a seat runs on.
mod capability;
pub(crate) mod fields;
pub(crate) mod resolve;
mod types;
pub use capability::{Capability, ModelApi, Support, Unlock};
use serde::{Deserialize, Serialize};
use std::fmt;
pub use types::{Harness, Isolation};

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
    /// `claude-code`.
    #[serde(rename = "claude-code")]
    ClaudeCode,
}

impl HarnessKind {
    /// Every kind, in the order the capability table lists them.
    pub const ALL: [Self; 4] = [
        Self::OpenHuman,
        Self::OpenClaw,
        Self::Codex,
        Self::ClaudeCode,
    ];

    /// How instances of this kind are shared when a harness entry does not
    /// say: one per deployment for `openclaw`, whose instance costs
    /// gigabytes, one per seat otherwise.
    #[must_use]
    pub const fn default_isolation(self) -> Isolation {
        match self {
            Self::OpenClaw => Isolation::Deployment,
            Self::OpenHuman | Self::Codex | Self::ClaudeCode => Isolation::Seat,
        }
    }
}

impl fmt::Display for HarnessKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::OpenHuman => "openhuman",
            Self::OpenClaw => "openclaw",
            Self::Codex => "codex",
            Self::ClaudeCode => "claude-code",
        })
    }
}

#[cfg(test)]
mod test;
