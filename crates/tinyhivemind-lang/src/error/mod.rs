//! Typed failures for package parsing, references and invariant validation.
use thiserror::Error;
/// Package operations return typed validation failures.
pub type Result<T> = std::result::Result<T, Error>;
/// An invalid portable package.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum Error {
    /// A mandatory or referenced document was absent.
    #[error("missing document {path}")]
    MissingDocument {
        /// Relative document path.
        path: String,
    },
    /// Canonical JSON did not match the typed schema.
    #[error("invalid json in {path}: {message}")]
    Json {
        /// Document path.
        path: String,
        /// Decoder explanation.
        message: String,
    },
    /// Unsupported wire schema or version.
    #[error("unsupported package schema or version")]
    Schema,
    /// Empty or duplicate identity in a section.
    #[error("invalid {kind} identity {id}")]
    Identity {
        /// Section name.
        kind: String,
        /// Invalid identity.
        id: String,
    },
    /// A declared reference did not resolve.
    #[error("unresolved {kind} reference {id}")]
    Reference {
        /// Target kind.
        kind: String,
        /// Missing target.
        id: String,
    },
    /// Unsafe host document path.
    #[error("unsafe relative path {path}")]
    Path {
        /// Rejected path.
        path: String,
    },
    /// Invalid bound or constitution cap.
    #[error("invalid policy bound {field}")]
    Bound {
        /// Rejected field.
        field: String,
    },
    /// Cyclic goal decomposition.
    #[error("cyclic goal decomposition at {id}")]
    GoalCycle {
        /// Goal on the cycle.
        id: String,
    },
    /// Malformed memory namespace.
    #[error("invalid memory namespace {namespace}")]
    Namespace {
        /// Rejected namespace.
        namespace: String,
    },
    /// Memory access violates host constraints.
    #[error("forbidden memory access {namespace}")]
    MemoryAccess {
        /// Protected or broadened namespace.
        namespace: String,
    },
    /// A shared context document has malformed or duplicate stable item markers.
    #[error("invalid context item markers in {path}")]
    Context {
        /// Rejected context document path.
        path: String,
    },
    /// Seat frontmatter disagrees with manifest metadata.
    #[error("frontmatter disagrees with manifest for {seat}")]
    Frontmatter {
        /// Seat identity.
        seat: String,
    },
}
