//! Why a deployment plan or the package behind it is refused.
/// Runtime-independent manifest failures; never contain resolved credentials.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// Invalid canonical JSON or unknown typed fields.
    #[error("invalid config json")]
    Json,
    /// Invalid YAML delimiters or metadata.
    #[error("invalid frontmatter in {0}")]
    Frontmatter(String),
    /// Filesystem read failed.
    #[error("cannot read config file {path}")]
    Io {
        /// Relative or requested path, without file contents.
        path: String,
        /// Underlying filesystem failure.
        #[source]
        source: std::io::Error,
    },
    /// Duplicate named declaration or membership.
    #[error("duplicate {section} id {id}")]
    DuplicateId {
        /// Declaration namespace.
        section: String,
        /// Repeated id.
        id: String,
    },
    /// Absent template reference.
    #[error("unknown profile {0}")]
    UnknownProfile(String),
    /// Absent authority reference.
    #[error("unknown permission profile {0}")]
    UnknownPermissionProfile(String),
    /// Absent seat reference.
    #[error("unknown seat {0}")]
    UnknownSeat(String),
    /// Absent hive reference.
    #[error("unknown hive {0}")]
    UnknownHive(String),
    /// Absent Markdown context reference.
    #[error("unknown context {0}")]
    UnknownContext(String),
    /// Empty role label.
    #[error("invalid role for seat {0}")]
    InvalidRole(String),
    /// Overlay broadens its inherited authority.
    #[error("permission widening in {0}")]
    PermissionWidening(String),
    /// Membership changes a live seat connection set.
    #[error("conflicting mcp connections for seat {0}; use separate seats")]
    ConflictingMcp(String),
    /// Invalid bounded concurrency.
    #[error("invalid width in {0}")]
    InvalidWidth(String),
    /// A credential field contains a literal.
    #[error("inline secret in {field}")]
    InlineSecret {
        /// Structural field path only.
        field: String,
    },
    /// Empty hive cannot run.
    #[error("empty hive {0}")]
    EmptyHive(String),
    /// Invalid declaration or native seat identifier.
    #[error("invalid id in {0}")]
    InvalidId(String),
    /// Invalid `OpenHuman` agent identity after applying the deployment rule.
    #[error("invalid OpenHuman agent id for seat {seat}: {agent_id:?}")]
    InvalidAgentId {
        /// Stable manifest seat key.
        seat: String,
        /// Derived or explicitly configured `OpenHuman` id.
        agent_id: String,
    },
    /// An `OpenHuman` agent identity collides with a built-in definition.
    #[error("OpenHuman agent id {agent_id:?} for seat {seat} is reserved")]
    ReservedAgentId {
        /// Stable manifest seat key.
        seat: String,
        /// Reserved `OpenHuman` id.
        agent_id: String,
    },
    /// Two configured seats resolve to the same `OpenHuman` identity.
    #[error("seats {first} and {second} resolve to the same OpenHuman agent id {agent_id:?}")]
    DuplicateAgentId {
        /// First seat resolving to the identity.
        first: String,
        /// Later seat resolving to the same identity.
        second: String,
        /// Colliding `OpenHuman` identity.
        agent_id: String,
    },
    /// A seat cannot use the host-wide default memory root.
    #[error("seat {0} cannot use the default shared memory root")]
    DefaultMemoryRoot(String),
    /// Nonpositive or impossible resource limit.
    #[error("invalid limit in {0}")]
    InvalidLimit(String),
    /// Malformed scheduling declaration.
    #[error("invalid workflow {0}")]
    InvalidWorkflow(String),
    /// Selector cannot be lowered by the adapter.
    #[error("unsupported setting {0}")]
    UnsupportedSetting(String),
    /// A hive package that does not check; carries every finding.
    #[error("hive package {root} has {} finding(s), first: {}", .diagnostics.len(), .diagnostics.first().map(ToString::to_string).unwrap_or_default())]
    Package {
        /// The package root as given.
        root: String,
        /// Every finding, located.
        diagnostics: Vec<tinyhivemind_lang::Diagnostic>,
    },
    /// Two packages declare one seat with different integrations.
    #[error("seat {0} is declared differently by two packages")]
    ConflictingSeat(String),
    /// Two packages declare one id differently in a shared section.
    #[error("{section} {id} is declared differently by two packages")]
    ConflictingDeclaration {
        /// The section, such as `runtime` or `models`.
        section: String,
        /// The id, or the section name for `runtime`.
        id: String,
    },
}
