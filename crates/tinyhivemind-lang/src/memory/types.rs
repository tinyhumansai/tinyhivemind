//! Memory declarations contain bindings and limits, never remembered content.
use crate::patch::PatchClass;
use serde::{Deserialize, Serialize};
use serde_json::Value;
/// Constitution in the portable hive wire format.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Constitution {
    /// Hard requirements the host evaluator must enforce.
    pub hard_constraints: Vec<String>,
    /// Maximum turn wall any package policy may request.
    pub max_turns: u64,
    /// Maximum concurrent width any package policy may request.
    pub max_width: usize,
    /// Edit classes the host permits for editor proposals.
    pub permitted_edits: Vec<PatchClass>,
    /// Identity IDs pinned read-only regardless of patch class.
    pub read_only_memories: Vec<String>,
    /// Evaluation or orchestrator namespace prefixes seats cannot access.
    pub protected_namespaces: Vec<String>,
    /// Held-out text markers forbidden in learning deltas.
    pub contamination_canaries: Vec<String>,
    /// Host evaluator configuration, immutable to editors.
    pub evaluation: Value,
    /// Host acceptance configuration, immutable to editors.
    pub acceptance: Value,
    /// Host logging configuration, immutable to editors.
    pub telemetry: Value,
}
impl Default for Constitution {
    fn default() -> Self {
        Self {
            hard_constraints: vec![],
            max_turns: 60,
            max_width: 8,
            permitted_edits: vec![],
            read_only_memories: vec![],
            protected_namespaces: vec![],
            contamination_canaries: vec![],
            evaluation: Value::Null,
            acceptance: Value::Null,
            telemetry: Value::Null,
        }
    }
}
/// `MemorySpec` in the portable hive wire format.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemorySpec {
    /// Namespace owning this package memory tree.
    pub root: String,
    /// Named agent or shared pool memories, without stored entries.
    pub identities: Vec<MemoryIdentity>,
    /// Seat assignments, read reaches and recall budgets.
    pub bindings: Vec<SeatMemory>,
}
impl Default for MemorySpec {
    fn default() -> Self {
        Self {
            root: "team:hive".into(),
            identities: vec![],
            bindings: vec![],
        }
    }
}
/// `MemoryIdentity` in the portable hive wire format.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryIdentity {
    /// Stable memory identity independent of seat identity.
    pub id: String,
    /// Qualified memory namespace using engine-compatible syntax.
    pub namespace: String,
    /// Whether this identity is private agent memory or a shared pool.
    pub kind: MemoryKind,
    /// Whether the host retains memory for one run or across runs.
    pub lifecycle: MemoryLifecycle,
    /// Whether the host must refuse remembered writes.
    pub read_only: bool,
    /// Optional identity whose namespace this child forks.
    pub fork_parent: Option<String>,
}
/// `SeatMemory` in the portable hive wire format.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SeatMemory {
    /// Stable seat identity receiving this binding.
    pub seat: String,
    /// Declared memory identity bound to the seat.
    pub identity: String,
    /// Bounded namespace reaches available during recall.
    pub reads: Vec<MemoryReach>,
    /// Session lifecycle events at which the host recalls memory.
    pub recall_at: Vec<RecallMoment>,
    /// Maximum characters the host may recall; nonzero.
    pub budget_chars: usize,
    /// Entry categories the host may store after activations.
    pub remember: Vec<EntryKind>,
}
/// `MemoryReach` in the portable hive wire format.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct MemoryReach {
    /// Exact namespace at which the read begins.
    pub at: String,
    /// Ancestor inheritance flag; packages require false to exclude global root.
    /// List each permitted ancestor as an explicit bounded reach instead.
    pub inherit: bool,
    /// Whether descendant namespaces participate in recall.
    pub descendants: bool,
}
/// Portable `MemoryKind` choice.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    /// Agent behavior.
    Agent,
    /// Pool behavior.
    Pool,
}
/// Portable `MemoryLifecycle` choice.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryLifecycle {
    /// Run behavior.
    Run,
    /// Hive behavior.
    Hive,
}
/// Portable `RecallMoment` choice.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RecallMoment {
    /// `SessionStart` behavior.
    SessionStart,
    /// Rejoin behavior.
    Rejoin,
    /// Compaction behavior.
    Compaction,
}
/// Portable `EntryKind` choice.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    /// Observation behavior.
    Observation,
    /// `FailedAttempt` behavior.
    FailedAttempt,
    /// Outcome behavior.
    Outcome,
    /// Note behavior.
    Note,
}
