//! Whether a started instance is exactly the one that was asked for.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// What a client requires of the instance it started.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Expected {
    pid: u32,
    version: String,
    tools: BTreeSet<String>,
}

/// An instance that met its [`Expected`].
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Started {
    /// The process that answers for the instance.
    pub pid: u32,
    /// Its version.
    pub version: String,
    /// Every tool the model sees, by the names it sees them under.
    pub tools: BTreeSet<String>,
}

/// How an instance differs from what was asked for.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Mismatch {
    /// Something else answers where the child should.
    #[error("process {found} answers, not the started {expected}")]
    Pid {
        /// The child.
        expected: u32,
        /// Who answers.
        found: u32,
    },
    /// The harness is not the pinned version.
    #[error("version {found} is running, not the pinned {expected}")]
    Version {
        /// The pinned version.
        expected: String,
        /// The running version.
        found: String,
    },
    /// The model would see another set of tools.
    #[error("the model would see other tools: missing {missing:?}, unexpected {unexpected:?}")]
    Tools {
        /// Required, but not offered.
        missing: Vec<String>,
        /// Offered, but not required, or offered more than once.
        unexpected: Vec<String>,
    },
}

impl Expected {
    /// The child `pid`, the pinned `version`, and exactly `tools`, by the
    /// names this harness shows its model.
    #[must_use]
    pub fn new(
        pid: u32,
        version: impl Into<String>,
        tools: impl IntoIterator<Item = String>,
    ) -> Self {
        Self {
            pid,
            version: version.into(),
            tools: tools.into_iter().collect(),
        }
    }

    /// Hold what the instance says of itself against this.
    ///
    /// # Errors
    /// The first [`Mismatch`], in the order pid, version, tools.
    pub fn verify(
        &self,
        pid: u32,
        version: &str,
        tools: impl IntoIterator<Item = String>,
    ) -> Result<Started, Mismatch> {
        if pid != self.pid {
            return Err(Mismatch::Pid {
                expected: self.pid,
                found: pid,
            });
        }
        if version != self.version {
            return Err(Mismatch::Version {
                expected: self.version.clone(),
                found: version.into(),
            });
        }
        let mut found = BTreeSet::new();
        let mut unexpected = BTreeSet::new();
        for tool in tools {
            let required = self.tools.contains(&tool);
            let repeated = !found.insert(tool.clone());
            if repeated || !required {
                unexpected.insert(tool);
            }
        }
        let missing: Vec<_> = self.tools.difference(&found).cloned().collect();
        if !missing.is_empty() || !unexpected.is_empty() {
            return Err(Mismatch::Tools {
                missing,
                unexpected: unexpected.into_iter().collect(),
            });
        }
        Ok(Started {
            pid,
            version: version.into(),
            tools: found,
        })
    }
}
