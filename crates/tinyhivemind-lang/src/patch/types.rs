//! Editor wire operations and typed failures.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The authority required for a proposal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PatchClass {
    /// Seat prompts and role charters.
    Prompt,
    /// Stable shared context items.
    Context,
    /// Memory identities, bindings, or host learning operations.
    Memory,
    /// Roles, seats, and desk membership.
    Roster,
    /// Hierarchy and referrals.
    Topology,
    /// Runtime policy settings.
    Policy,
    /// Goals and missions.
    Goal,
}

/// An atomic proposal, authorized under one edit class.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    /// Authority required by every operation.
    pub class: PatchClass,
    /// Operations applied in order against a private candidate.
    pub operations: Vec<Operation>,
}

/// A preconditioned editor operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    /// Mutate a manifest or memory JSON Pointer.
    Json {
        /// Pointer begins with `/manifest/` or `/memory/`.
        pointer: String,
        /// Expected current value; `None` requires absence.
        before: Option<Value>,
        /// New value; `None` removes the current value.
        after: Option<Value>,
    },
    /// Create or retire a side document alongside its typed references.
    Document {
        /// Seat, role, or shared context Markdown path.
        path: String,
        /// Exact prior document; absent for creation.
        before: Option<String>,
        /// New document; absent for retirement. Existing rewrites are forbidden.
        after: Option<String>,
    },
    /// Replace one prompt body, preserving its structured frontmatter.
    Prompt {
        /// A seat or role Markdown path.
        path: String,
        /// Exact expected body.
        before: String,
        /// Proposed body.
        after: String,
    },
    /// Add, replace, or remove a stable Markdown item.
    Context {
        /// Shared context Markdown path.
        path: String,
        /// Stable marker identity (`<!-- item:identity -->`).
        item: String,
        /// Exact previous item body; `None` requires absence.
        before: Option<String>,
        /// New item body; `None` removes the item.
        after: Option<String>,
    },
    /// Return an auditable learning operation to the host; no store is opened.
    Learning {
        /// Declared writable memory identity.
        identity: String,
        /// Stable host entry identity.
        entry: String,
        /// Host watermark the proposal read.
        watermark: u64,
        /// Expected previous entry; checked by the host at this watermark.
        before: Option<String>,
        /// New entry contents; `None` requests forgetting.
        after: Option<String>,
    },
}

/// Result of a guarded proposal; the host persists and activates it.
#[derive(Clone, Debug, PartialEq)]
pub struct AppliedPatch {
    /// Validated candidate package.
    pub candidate: crate::Package,
    /// Learning operations for the host's memory transaction.
    pub learning: Vec<Operation>,
}

/// Proposal rejection with no partially applied state.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An operation exceeded the proposal's authority.
    #[error("operation is outside edit class at {0}")]
    ClassBoundary(String),
    /// The constitution disallows this class.
    #[error("edit class is not permitted")]
    ForbiddenClass,
    /// The proposal tried to modify immutable judging settings.
    #[error("immutable constitution or evaluation settings changed")]
    Immutable,
    /// A JSON Pointer was malformed or its parent absent.
    #[error("invalid JSON pointer {0}")]
    Pointer(String),
    /// The observed package does not match a proposal precondition.
    #[error("stale proposal at {0}")]
    Stale(String),
    /// Stable Markdown markers were malformed or duplicated.
    #[error("invalid stable Markdown item {0}")]
    Item(String),
    /// A declared read-only memory or its binding changed.
    #[error("read-only memory changed: {0}")]
    ReadOnly(String),
    /// Held-out contents appeared in the candidate or learning.
    #[error("candidate contains a contamination canary")]
    Contamination,
    /// Learning targeted an unknown identity or invalid entry.
    #[error("invalid learning target {0}")]
    Learning(String),
    /// A guarded candidate differs from the actual proposal result.
    #[error("candidate does not match proposal")]
    CandidateMismatch,
    /// Package validation rejected the full candidate.
    #[error("invalid candidate: {0}")]
    Validation(#[from] crate::Error),
    /// A nested host-neutral policy field was not part of its typed schema.
    #[error("unknown nested schema field in proposal")]
    UnknownField,
    /// Conversion to or from the package wire format failed.
    #[error("package wire conversion failed: {0}")]
    Wire(#[from] serde_json::Error),
}

/// Result of a patch operation.
pub type Result<T> = std::result::Result<T, Error>;
