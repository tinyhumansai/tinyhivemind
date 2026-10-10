//! Typed package manifests and caller-supplied document parsing.
use crate::{Constitution, MemorySpec};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use tinyhivemind_core::{
    aside::AsidePolicy,
    desk::Desk,
    driver::ConductPolicy,
    embed::RoutingPolicy,
    hive::{DirectoryPolicy, DivisionPolicy, EpisodePolicy, ExchangePolicy},
    runtime::{ApprovalPolicy, BrevityPolicy, DigestPolicy, ReferralPolicy},
};
/// Package in the portable hive wire format.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Package {
    /// Typed canonical hive definition.
    pub manifest: Manifest,
    /// Immutable host-pinned constraints and evaluator settings.
    pub constitution: Constitution,
    /// Memory identities and binding settings without contents.
    pub memory: MemorySpec,
    /// Markdown side documents keyed by safe relative path.
    pub documents: BTreeMap<String, String>,
}
/// Manifest in the portable hive wire format.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Canonical supported schema identifier.
    #[serde(rename = "$schema")]
    pub schema: String,
    /// Wire format version, currently one.
    pub version: u32,
    /// Stable case-sensitive identity within this section.
    pub id: String,
    /// Human-readable hive name.
    pub name: String,
    /// Independent organizational roles and their charter documents.
    pub roles: Vec<Role>,
    /// Agents declared for host factory creation.
    pub seats: Vec<Seat>,
    /// Ordered conversations using existing core desk values.
    pub desks: Vec<Desk>,
    /// Task decomposition and host-evaluated acceptance criteria.
    pub goals: Vec<Goal>,
    /// Work obligations binding roles to goal identities.
    pub missions: Vec<Mission>,
    /// Directed desk hierarchy; cycles are permitted.
    pub topology: Vec<Link>,
    /// Directed channels over which desks may refer work.
    pub referrals: Vec<Link>,
    /// Relative Markdown paths under `context/` for shared itemized documents.
    pub context: Vec<String>,
    /// Existing core policies, frozen in this activation snapshot.
    pub policies: Policies,
    /// Host-neutral scheduling settings.
    pub coordinator: CoordinatorSettings,
}
impl Default for Manifest {
    fn default() -> Self {
        Self {
            schema: "https://tinyhumans.ai/schemas/hive/v1".into(),
            version: 1,
            id: "hive".into(),
            name: "Hive".into(),
            roles: vec![],
            seats: vec![],
            desks: vec![],
            goals: vec![],
            missions: vec![],
            topology: vec![],
            referrals: vec![],
            context: vec![],
            policies: Policies::default(),
            coordinator: CoordinatorSettings::default(),
        }
    }
}
/// Role in the portable hive wire format.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Role {
    /// Stable case-sensitive identity within this section.
    pub id: String,
    /// Relative Markdown path under `roles/` for this independent charter.
    pub charter: String,
}
/// Seat in the portable hive wire format.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Seat {
    /// Stable case-sensitive identity within this section.
    pub id: String,
    /// Human-readable seat label.
    pub label: String,
    /// Identity of the role this seat fills.
    pub role: String,
    /// Relative Markdown path under `seats/` for this seat prompt.
    pub prompt: String,
    /// Host-defined factory template name; never credentials.
    pub template: String,
    /// Host-defined nonsecret factory configuration.
    pub config: Value,
    /// Optional host model selection.
    pub model: Option<String>,
    /// Explicit tool access policy for the host to enforce.
    pub tools: ToolAccess,
    /// Host-defined MCP server configuration names.
    pub mcp: Vec<String>,
    /// Skill names installed by the host.
    pub skills: Vec<String>,
}
/// `ToolAccess` in the portable hive wire format.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ToolAccess {
    /// Tool names the host permits.
    pub allow: Vec<String>,
    /// Tool names the host forbids.
    pub deny: Vec<String>,
}
/// Goal in the portable hive wire format.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Goal {
    /// Stable case-sensitive identity within this section.
    pub id: String,
    /// Human-readable purpose.
    pub description: String,
    /// Criteria checked by the host evaluator.
    pub acceptance: Vec<String>,
    /// Child goal identities; decomposition must be acyclic.
    pub subgoals: Vec<String>,
}
/// Mission in the portable hive wire format.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Mission {
    /// Stable case-sensitive identity within this section.
    pub id: String,
    /// Identity of the role this seat fills.
    pub role: String,
    /// Task decomposition and host-evaluated acceptance criteria.
    pub goals: Vec<String>,
}
/// Link in the portable hive wire format.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Link {
    /// Source desk identity.
    pub from: String,
    /// Target desk identity.
    pub to: String,
}
/// Policies in the portable hive wire format.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policies {
    /// Declared episode.
    pub episode: EpisodePolicy,
    /// Declared routing.
    pub routing: RoutingPolicy,
    /// Declared approval.
    pub approval: ApprovalPolicy,
    /// Declared aside.
    pub aside: AsidePolicy,
    /// Declared referral.
    pub referral: ReferralPolicy,
    /// Declared digest.
    pub digest: DigestPolicy,
    /// Declared brevity.
    pub brevity: BrevityPolicy,
    /// Declared directory.
    pub directory: DirectoryPolicy,
    /// Declared exchange.
    pub exchange: ExchangePolicy,
    /// Declared division.
    pub division: DivisionPolicy,
    /// Declared conduct.
    pub conduct: ConductPolicy,
}
impl Default for Policies {
    fn default() -> Self {
        Self {
            episode: EpisodePolicy {
                round_width: 1,
                ..EpisodePolicy::default()
            },
            routing: RoutingPolicy {
                minimum_confidence: tinyhivemind_core::runtime::responder::Probability::ZERO,
                high_impact_minimum_confidence:
                    tinyhivemind_core::runtime::responder::Probability::ZERO,
                clarification_threshold: tinyhivemind_core::runtime::responder::Probability::ZERO,
                high_impact_threshold: tinyhivemind_core::runtime::responder::Probability::ZERO,
                round_width: 1,
                choice_option_limit: 8,
            },
            approval: ApprovalPolicy {
                enabled: false,
                default: tinyhivemind_core::approval::DefaultVerdict::Deny,
                rules: vec![],
                approver: tinyhivemind_core::approval::ApproverRule::Absent,
                allow_grants: false,
                max_grant_ttl: None,
            },
            aside: AsidePolicy::default(),
            referral: ReferralPolicy::default(),
            digest: DigestPolicy::default(),
            brevity: BrevityPolicy::default(),
            directory: DirectoryPolicy::default(),
            exchange: ExchangePolicy::default(),
            division: DivisionPolicy::default(),
            conduct: ConductPolicy::default(),
        }
    }
}
/// `CoordinatorSettings` in the portable hive wire format.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CoordinatorSettings {
    /// Declared round width.
    pub round_width: usize,
    /// Declared conduct policy.
    pub conduct_policy: ConductPolicy,
    /// Declared broadcast budget.
    pub broadcast_budget: Option<u32>,
    /// Declared retention.
    pub retention: RetentionSettings,
}
impl Default for CoordinatorSettings {
    fn default() -> Self {
        Self {
            round_width: 1,
            conduct_policy: ConductPolicy::default(),
            broadcast_budget: None,
            retention: RetentionSettings::default(),
        }
    }
}
/// `RetentionSettings` in the portable hive wire format.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct RetentionSettings {
    /// Declared settled episodes.
    pub settled_episodes: Option<usize>,
    /// Declared delivered.
    pub delivered: Option<usize>,
    /// Declared interrupted.
    pub interrupted: Option<usize>,
    /// Declared pending per agent.
    pub pending_per_agent: Option<usize>,
}
