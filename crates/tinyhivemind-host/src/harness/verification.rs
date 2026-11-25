//! Whether a started instance is exactly the one that was asked for.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// What a client requires of the instance it started.
///
/// ```
/// use serde_json::json;
/// use tinyhivemind_host::harness::{Expected, Observed};
///
/// let expected = Expected::new(42, "1.2.0")
///     .tools(["hivemind_post".to_owned()])
///     .setting("memory", json!(false));
/// let observed = Observed::new(42, "1.2.0")
///     .tools(["hivemind_post".to_owned()])
///     .setting("memory", json!(true));
/// assert!(expected.verify(&observed).is_err());
/// ```
#[derive(Clone, Eq, PartialEq)]
pub struct Expected {
    pid: u32,
    version: String,
    tools: BTreeSet<String>,
    settings: BTreeMap<String, Value>,
}

/// What an instance says of itself once it is up.
#[derive(Clone, Eq, PartialEq)]
pub struct Observed {
    pid: u32,
    version: String,
    tools: Vec<String>,
    settings: BTreeMap<String, Value>,
}

/// An instance that met its [`Expected`].
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[non_exhaustive]
pub struct Started {
    /// The process that answers for the instance.
    pub pid: u32,
    /// Its version.
    pub version: String,
    /// Every tool the model sees, by the names it sees them under.
    pub tools: BTreeSet<String>,
    /// The settings that were compared, by name.
    pub settings: BTreeSet<String>,
}

/// How an instance differs from what was asked for.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[non_exhaustive]
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
    /// The effective config is not the one that was written. Settings are
    /// named and their values are not, as they may be secret.
    #[error(
        "the effective config is another one: missing {missing:?}, different {different:?}, unexpected {unexpected:?}"
    )]
    Settings {
        /// Required, but not there.
        missing: Vec<String>,
        /// There with another value.
        different: Vec<String>,
        /// There, but not required.
        unexpected: Vec<String>,
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
    /// The child `pid`, running the pinned `version`, with no tool and no
    /// setting to compare.
    #[must_use]
    pub fn new(pid: u32, version: impl Into<String>) -> Self {
        Self {
            pid,
            version: version.into(),
            tools: BTreeSet::new(),
            settings: BTreeMap::new(),
        }
    }

    /// Exactly `tools`, by the names this harness shows its model.
    #[must_use]
    pub fn tools(mut self, tools: impl IntoIterator<Item = String>) -> Self {
        self.tools = tools.into_iter().collect();
        self
    }

    /// The setting `name`, as it was written. The name is the client's own,
    /// and so is the choice of what to compare.
    #[must_use]
    pub fn setting(mut self, name: impl Into<String>, value: Value) -> Self {
        self.settings.insert(name.into(), value);
        self
    }

    /// Hold `observed` against this.
    ///
    /// # Errors
    /// The first [`Mismatch`], in the order pid, version, settings, tools.
    pub fn verify(&self, observed: &Observed) -> Result<Started, Mismatch> {
        if observed.pid != self.pid {
            return Err(Mismatch::Pid {
                expected: self.pid,
                found: observed.pid,
            });
        }
        if observed.version != self.version {
            return Err(Mismatch::Version {
                expected: self.version.clone(),
                found: observed.version.clone(),
            });
        }
        self.verify_settings(&observed.settings)?;
        Ok(Started {
            pid: observed.pid,
            version: observed.version.clone(),
            tools: self.verify_tools(&observed.tools)?,
            settings: observed.settings.keys().cloned().collect(),
        })
    }

    fn verify_settings(&self, observed: &BTreeMap<String, Value>) -> Result<(), Mismatch> {
        let absent = |names: &BTreeMap<String, Value>, from: &BTreeMap<String, Value>| {
            let absent = names.keys().filter(|name| !from.contains_key(*name));
            absent.cloned().collect::<Vec<_>>()
        };
        let missing = absent(&self.settings, observed);
        let unexpected = absent(observed, &self.settings);
        let different: Vec<_> = self
            .settings
            .iter()
            .filter(|(name, value)| observed.get(*name).is_some_and(|found| found != *value))
            .map(|(name, _)| name.clone())
            .collect();
        if missing.is_empty() && different.is_empty() && unexpected.is_empty() {
            return Ok(());
        }
        Err(Mismatch::Settings {
            missing,
            different,
            unexpected,
        })
    }

    fn verify_tools(&self, observed: &[String]) -> Result<BTreeSet<String>, Mismatch> {
        let mut found = BTreeSet::new();
        let mut unexpected = BTreeSet::new();
        for tool in observed {
            let required = self.tools.contains(tool);
            let repeated = !found.insert(tool.clone());
            if repeated || !required {
                unexpected.insert(tool.clone());
            }
        }
        let missing: Vec<_> = self.tools.difference(&found).cloned().collect();
        if missing.is_empty() && unexpected.is_empty() {
            return Ok(found);
        }
        Err(Mismatch::Tools {
            missing,
            unexpected: unexpected.into_iter().collect(),
        })
    }
}

impl Observed {
    /// The process that answers, and its version.
    #[must_use]
    pub fn new(pid: u32, version: impl Into<String>) -> Self {
        Self {
            pid,
            version: version.into(),
            tools: Vec::new(),
            settings: BTreeMap::new(),
        }
    }

    /// Every tool the model sees, as often as it sees it.
    #[must_use]
    pub fn tools(mut self, tools: impl IntoIterator<Item = String>) -> Self {
        self.tools = tools.into_iter().collect();
        self
    }

    /// The setting `name`, as the running instance has it. Whatever the
    /// instance could do that nobody asked for, such as reach another
    /// provider, belongs here too: a setting that is not expected is refused.
    #[must_use]
    pub fn setting(mut self, name: impl Into<String>, value: Value) -> Self {
        self.settings.insert(name.into(), value);
        self
    }
}

impl fmt::Debug for Expected {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Expected")
            .field("pid", &self.pid)
            .field("version", &self.version)
            .field("tools", &self.tools)
            .field("settings", &self.settings.keys())
            .finish()
    }
}

impl fmt::Debug for Observed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Observed")
            .field("pid", &self.pid)
            .field("version", &self.version)
            .field("tools", &self.tools)
            .field("settings", &self.settings.keys())
            .finish()
    }
}
