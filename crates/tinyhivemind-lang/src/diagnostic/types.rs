//! The diagnostic wire: where a finding lives, which rule it breaks, and why.

use serde::{Deserialize, Serialize};
use std::fmt;

/// One finding against a hive package, located like a compiler error.
///
/// Displays as `file:pointer: rule: message`, or `file: rule: message` when
/// there is no pointer.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    /// Package-relative path of the file the finding concerns, such as
    /// `hive.json` or `seats/solver.md`.
    pub file: String,
    /// JSON Pointer inside `file`, or `None` for a Markdown file or a finding
    /// about the whole document.
    pub pointer: Option<String>,
    /// The rule family the finding breaks.
    pub rule: Rule,
    /// Lowercase explanation without trailing punctuation.
    pub message: String,
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.pointer {
            Some(pointer) => write!(
                f,
                "{}:{pointer}: {}: {}",
                self.file, self.rule, self.message
            ),
            None => write!(f, "{}: {}: {}", self.file, self.rule, self.message),
        }
    }
}

/// The rule family a [`Diagnostic`] reports.
///
/// The first thirteen mirror the package [`crate::Error`] variants; the rest are
/// the rules [`crate::check()`] adds for an edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Rule {
    /// Unsupported `$schema` or `version` ([`crate::Error::Schema`]).
    Schema,
    /// An empty or duplicate identity ([`crate::Error::Identity`]).
    Identity,
    /// A reference that does not resolve ([`crate::Error::Reference`]).
    Reference,
    /// An unsafe or misplaced document path ([`crate::Error::Path`]).
    Path,
    /// A policy outside its bound or the constitution's caps
    /// ([`crate::Error::Bound`]).
    Bound,
    /// A cyclic goal decomposition ([`crate::Error::GoalCycle`]).
    GoalCycle,
    /// A malformed memory namespace ([`crate::Error::Namespace`]).
    Namespace,
    /// A memory identity or reach the constitution forbids
    /// ([`crate::Error::MemoryAccess`]).
    MemoryAccess,
    /// Malformed context item markers, or a context edit that item deltas
    /// cannot express ([`crate::Error::Context`]).
    Context,
    /// Frontmatter that is malformed or disagrees with the manifest
    /// ([`crate::Error::Frontmatter`]).
    Frontmatter,
    /// A canonical file that is not valid JSON for its schema
    /// ([`crate::Error::Json`]).
    Json,
    /// A required or referenced document that is absent
    /// ([`crate::Error::MissingDocument`]).
    MissingDocument,
    /// An integration setting the harness cannot run as written
    /// ([`crate::Error::Integration`]).
    Integration,
    /// A change of an edit class the constitution does not permit.
    ForbiddenClass,
    /// A change to the constitution, a protected key, or a frontmatter.
    Immutable,
    /// A contamination canary in the edited package.
    Contamination,
    /// A change to a memory identity, or its bindings, pinned read-only.
    ReadOnly,
    /// A change no edit class owns, or a file that is not part of a package.
    Unowned,
    /// A finding against the original package rather than the edit.
    Original,
    /// No order of the per-class changes is valid at every step.
    Sequence,
}

impl Rule {
    /// The rule's wire name, such as `goal_cycle`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Schema => "schema",
            Self::Identity => "identity",
            Self::Reference => "reference",
            Self::Path => "path",
            Self::Bound => "bound",
            Self::GoalCycle => "goal_cycle",
            Self::Namespace => "namespace",
            Self::MemoryAccess => "memory_access",
            Self::Context => "context",
            Self::Frontmatter => "frontmatter",
            Self::Json => "json",
            Self::MissingDocument => "missing_document",
            Self::Integration => "integration",
            Self::ForbiddenClass => "forbidden_class",
            Self::Immutable => "immutable",
            Self::Contamination => "contamination",
            Self::ReadOnly => "read_only",
            Self::Unowned => "unowned",
            Self::Original => "original",
            Self::Sequence => "sequence",
        }
    }
}

impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl From<&crate::Error> for Rule {
    fn from(error: &crate::Error) -> Self {
        use crate::Error;
        match error {
            Error::MissingDocument { .. } => Self::MissingDocument,
            Error::Json { .. } => Self::Json,
            Error::Schema => Self::Schema,
            Error::Identity { .. } => Self::Identity,
            Error::Reference { .. } => Self::Reference,
            Error::Path { .. } => Self::Path,
            Error::Bound { .. } => Self::Bound,
            Error::GoalCycle { .. } => Self::GoalCycle,
            Error::Namespace { .. } => Self::Namespace,
            Error::MemoryAccess { .. } => Self::MemoryAccess,
            Error::Context { .. } => Self::Context,
            Error::Frontmatter { .. } => Self::Frontmatter,
            Error::Integration { .. } => Self::Integration,
        }
    }
}
