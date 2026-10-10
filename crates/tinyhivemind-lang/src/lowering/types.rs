//! Translate validated packages into existing core values and host requests.
use crate::{
    CoordinatorSettings, Goal, Link, MemorySpec, Mission, Policies, SeatMemory, ToolAccess,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tinyhivemind_core::{desk::Desk, embed::RouteCandidate};
/// A complete host-neutral activation snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct LoweredHive {
    /// Stable package identity for host activation and lineage association.
    pub id: String,
    /// Human-readable hive name.
    pub name: String,
    /// Goal decomposition and host-evaluated acceptance criteria.
    pub goals: Vec<Goal>,
    /// Work obligations binding roles to goal identities.
    pub missions: Vec<Mission>,
    /// Directed desk hierarchy; cycles are preserved.
    pub topology: Vec<Link>,
    /// Directed desk referral channels.
    pub referrals: Vec<Link>,
    /// Declared shared context paths and their complete document contents.
    pub context: BTreeMap<String, String>,
    /// Existing desk values.
    pub desks: Vec<Desk>,
    /// Routing candidates with declared roles preserved.
    pub candidates: Vec<RouteCandidate>,
    /// Host seat creation inputs.
    pub seats: Vec<LoweredSeat>,
    /// Existing core policy values.
    pub policies: Policies,
    /// Host-neutral coordinator options.
    pub coordinator: CoordinatorSettings,
    /// Memory identities and bindings, without contents.
    pub memory: MemorySpec,
}
/// Resolved identity plus runtime memory configuration retained for the host.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LoweredMemoryBinding {
    /// Resolved memory namespace identifying the agent or pool.
    pub agent_id: String,
    /// Full resolved namespace retained for host capability checks.
    pub namespace: String,
    /// Agent or shared pool identity.
    pub kind: crate::MemoryKind,
    /// Identity lifecycle, freeze and fork requirements.
    pub identity: crate::MemoryIdentity,
    /// Root namespace supplied to the runtime.
    pub root: String,
    /// Recall, remember and read settings the host must install.
    pub settings: SeatMemory,
}
/// Host-owned seat configuration, without a runner or credentials.
#[derive(Clone, Debug, PartialEq)]
pub struct LoweredSeat {
    /// Seat identity.
    pub id: String,
    /// Display label.
    pub label: String,
    /// Host factory template.
    pub template: String,
    /// Host nonsecret configuration.
    pub config: serde_json::Value,
    /// Prompt body, without frontmatter.
    pub prompt: String,
    /// Optional model name.
    pub model: Option<String>,
    /// Explicit tool access.
    pub tools: ToolAccess,
    /// MCP configuration names.
    pub mcp: Vec<String>,
    /// Skill names.
    pub skills: Vec<String>,
    /// Resolved memory identity and preserved host settings.
    pub memory: Option<LoweredMemoryBinding>,
}
