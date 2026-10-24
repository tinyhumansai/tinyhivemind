//! Wire types for the package manifest (`hive.json`) and the whole package.
//!
//! Every struct denies unknown fields, so a misspelled key is a decode error
//! rather than a silently ignored setting. Identities are opaque,
//! case-sensitive strings: [`crate::validate`] rejects an empty (or
//! whitespace-only) identity and a duplicate within the same section.
use super::integration::{
    ExecTool, Hook, JevSettings, Limits, McpServer, Model, PermissionSet, Profile, Provider,
    RuntimeSettings, SessionSettings, Skill, Toolpack, Workflow,
};
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
/// A complete hive package: the three canonical JSON files plus its Markdown.
///
/// [`crate::parse`] builds one from a document map keyed by `hive.json`,
/// `constitution.json`, `memory.json` and relative Markdown paths. The struct
/// also serializes as a single JSON object with these four fields, which is
/// the form a host can store or hand across a process boundary in one file.
/// [`Package::default`] is an empty but valid package.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Package {
    /// The decoded `hive.json`: what the hive is made of.
    pub manifest: Manifest,
    /// The decoded `constitution.json`: caps and judging settings an editor
    /// proposal may never change.
    pub constitution: Constitution,
    /// The decoded `memory.json`: memory identities and seat bindings, never
    /// remembered contents.
    pub memory: MemorySpec,
    /// Markdown documents keyed by safe relative path.
    ///
    /// [`crate::parse`] keeps only keys ending in `.md` (any case); every other
    /// non-canonical file is dropped. Documents no manifest entry references,
    /// such as a `README.md`, are accepted and carried along, and so take part
    /// in [`crate::lineage::content_id`].
    pub documents: BTreeMap<String, String>,
}
/// The typed hive definition decoded from `hive.json`.
///
/// Only `$schema`, `version`, `id`, `name` and `seats` are required on the
/// wire. Every list section defaults to empty; `policies` is overlaid on
/// [`Policies::default`] (a JSON object merges key by key into the default,
/// recursively, and any other value replaces it), and any omitted
/// `coordinator` field takes its [`CoordinatorSettings::default`] value. A
/// manifest therefore names only the settings it changes. Serialization
/// always writes every field, so a stored manifest is complete.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// The `$schema` key; must be exactly
    /// `https://tinyhumans.ai/schemas/hive/v1` or validation fails with
    /// [`crate::Error::Schema`].
    #[serde(rename = "$schema")]
    pub schema: String,
    /// Wire format version; only `1` is accepted.
    pub version: u32,
    /// The hive's identity; must be non-blank. Lowering carries it as
    /// [`crate::LoweredHive::id`].
    pub id: String,
    /// Human-readable hive name, carried unchanged into lowering.
    pub name: String,
    /// The roles seats and missions refer to, each with its own charter.
    #[serde(default)]
    pub roles: Vec<Role>,
    /// The agents the host creates, one per seat.
    pub seats: Vec<Seat>,
    /// Core desk values; every member must name a declared seat, at most once.
    ///
    /// A desk's `description` may be omitted and defaults to `None`.
    #[serde(default, deserialize_with = "super::defaults::desks")]
    pub desks: Vec<Desk>,
    /// Goals, which may decompose into subgoals; the decomposition must be
    /// acyclic.
    #[serde(default)]
    pub goals: Vec<Goal>,
    /// Missions assigning goals to roles.
    #[serde(default)]
    pub missions: Vec<Mission>,
    /// Directed parent-to-child desk hierarchy; both ends must name declared
    /// desks, and cycles are permitted.
    #[serde(default)]
    pub topology: Vec<Link>,
    /// Directed channels along which one desk may refer work to another; both
    /// ends must name declared desks.
    #[serde(default)]
    pub referrals: Vec<Link>,
    /// Paths of shared context documents. Each must start with `context/`,
    /// end in `.md`, be present in [`Package::documents`], and carry
    /// well-formed, unique `<!-- item:id -->` markers.
    #[serde(default)]
    pub context: Vec<String>,
    /// Core policy values the host installs on activation, bounded by the
    /// constitution.
    #[serde(default, deserialize_with = "super::defaults::policies")]
    pub policies: Policies,
    /// Scheduling settings for a host coordinator.
    #[serde(default)]
    pub coordinator: CoordinatorSettings,
    /// The runtime the seats run on, and the defaults they inherit.
    #[serde(default)]
    pub runtime: RuntimeSettings,
    /// Inference providers.
    #[serde(default)]
    pub providers: Vec<Provider>,
    /// Models seats run on.
    #[serde(default)]
    pub models: Vec<Model>,
    /// MCP server connections.
    #[serde(default)]
    pub mcp: Vec<McpServer>,
    /// Skill bundles under `skills/`.
    #[serde(default)]
    pub skills: Vec<Skill>,
    /// Executable tools; see `docs/specs/executable-tools.md`.
    #[serde(default)]
    pub tools: Vec<ExecTool>,
    /// Named bundles of tools, MCP servers, skills and harness tool groups.
    #[serde(default)]
    pub toolpacks: Vec<Toolpack>,
    /// Permission sets.
    #[serde(default)]
    pub permissions: Vec<PermissionSet>,
    /// Reusable seat integrations.
    #[serde(default)]
    pub profiles: Vec<Profile>,
    /// Executable tools bound to events.
    #[serde(default)]
    pub hooks: Vec<Hook>,
    /// Agent session settings.
    #[serde(default)]
    pub session: SessionSettings,
    /// Jev tool-search settings.
    #[serde(default)]
    pub jev: JevSettings,
    /// Feature flags. `mcp`, `skills`, `script_tools` and `hooks` set to
    /// `false` switch that integration off; every flag reaches executable
    /// tools as `THM_FEATURE_<NAME>`.
    #[serde(default)]
    pub features: BTreeMap<String, bool>,
    /// Scheduled prompts.
    #[serde(default)]
    pub workflows: Vec<Workflow>,
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
            runtime: RuntimeSettings::default(),
            providers: vec![],
            models: vec![],
            mcp: vec![],
            skills: vec![],
            tools: vec![],
            toolpacks: vec![],
            permissions: vec![],
            profiles: vec![],
            hooks: vec![],
            session: SessionSettings::default(),
            jev: JevSettings::default(),
            features: BTreeMap::new(),
            workflows: vec![],
        }
    }
}
/// One role: a responsibility seats fill and missions are assigned to.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Role {
    /// The role's identity, unique among roles.
    pub id: String,
    /// Path of the role's charter; must start with `roles/`, end in `.md`, and
    /// be present in [`Package::documents`].
    ///
    /// The charter may open with JSON frontmatter between `---\n` and
    /// `\n---\n`; if it does, the object may hold only `id` and `charter`, with
    /// the same values as here. The charter body, without frontmatter, becomes
    /// the routing candidate description of every seat filling this role.
    pub charter: String,
}
/// One seat: an agent the host creates and places on desks.
///
/// On the wire only `id`, `role`, `prompt` and `template` are required:
/// `label` defaults to the seat's `id`, `config` to `{}`, `model`, `profile`,
/// `agent_id` and `permissions` to `null`, `limits` to no limits, and every
/// list to empty. Unknown fields are rejected.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Default)]
#[serde(from = "super::defaults::SeatWire")]
pub struct Seat {
    /// The seat's identity, unique among seats; desk members, memory bindings
    /// and routing candidates refer to it.
    pub id: String,
    /// Display label, also used as the routing candidate label; defaults to
    /// the seat's `id` when omitted.
    pub label: String,
    /// Identity of the [`Role`] this seat fills; must name a declared role.
    pub role: String,
    /// Path of the seat's prompt; must start with `seats/`, end in `.md`, and
    /// be present in [`Package::documents`].
    ///
    /// The prompt may open with JSON frontmatter between `---\n` and
    /// `\n---\n`. Every key it holds must name a field of this seat, or one of
    /// the seat's memory binding settings (`reads`, `recall_at`,
    /// `budget_chars`, `remember`, and `memory` for the bound identity), with
    /// exactly the manifest's value; otherwise validation fails with
    /// [`crate::Error::Frontmatter`]. Lowering hands the host the body alone.
    pub prompt: String,
    /// Name of the host factory template that creates the agent; must be
    /// non-blank. Never a credential.
    pub template: String,
    /// Opaque, nonsecret factory configuration passed through to the host;
    /// `{}` when omitted.
    pub config: Value,
    /// Id of the [`Model`] the seat runs on; `None` inherits the profile's,
    /// then the runtime's.
    pub model: Option<String>,
    /// Tool access the host must enforce; the allow list also becomes the
    /// routing candidate's capabilities.
    pub tools: ToolAccess,
    /// Ids of the [`McpServer`]s the seat connects to.
    pub mcp: Vec<String>,
    /// Ids of the [`Skill`]s the seat has.
    pub skills: Vec<String>,
    /// Id of the [`Profile`] the seat starts from.
    pub profile: Option<String>,
    /// Harness agent id; `None` derives `<deployment_id>-<seat id>`.
    pub agent_id: Option<String>,
    /// Ids of the [`Toolpack`]s the seat receives.
    pub toolpacks: Vec<String>,
    /// Ids of the [`Hook`]s that run on the seat's turns.
    pub hooks: Vec<String>,
    /// Id of the [`PermissionSet`] narrowing the seat in this hive.
    pub permissions: Option<String>,
    /// Turn limits.
    pub limits: Limits,
    /// Extra `context/*.md` paths composed into the seat's prompt.
    pub context: Vec<String>,
}
/// A seat's tool access, enforced by the host; this crate only carries it.
///
/// Both lists default to empty when omitted, as does the whole value.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct ToolAccess {
    /// Tool names the seat may call.
    pub allow: Vec<String>,
    /// Tool names the seat must not call.
    pub deny: Vec<String>,
}
/// One goal, optionally decomposed into subgoals.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Goal {
    /// The goal's identity, unique among goals.
    pub id: String,
    /// What the goal is for, in prose.
    pub description: String,
    /// Acceptance criteria, opaque to this crate and judged by the host.
    /// Unlike `acceptance` elsewhere in a package, these are editable by a
    /// [`crate::patch::PatchClass::Goal`] proposal. Empty when omitted.
    #[serde(default)]
    pub acceptance: Vec<String>,
    /// Identities of child goals; each must name a declared goal, and the
    /// whole decomposition must be acyclic ([`crate::Error::GoalCycle`]).
    /// Empty when omitted.
    #[serde(default)]
    pub subgoals: Vec<String>,
}
/// One mission: an obligation assigning goals to a role.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Mission {
    /// The mission's identity, unique among missions.
    pub id: String,
    /// Identity of the [`Role`] that carries the mission; must name a declared
    /// role (a role, not a seat).
    pub role: String,
    /// Identities of the goals the role is responsible for; each must name a
    /// declared [`Goal`].
    pub goals: Vec<String>,
}
/// A directed edge between two desks, used by topology and referrals.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Link {
    /// Identity of the desk the edge leaves; must name a declared desk.
    pub from: String,
    /// Identity of the desk the edge enters; must name a declared desk.
    pub to: String,
}
/// The core policy values a hive runs under, each in its core wire form.
///
/// Validation bounds them against the [`Constitution`]; a failing bound is
/// reported as [`crate::Error::Bound`] naming the field. The default is a
/// valid sequential configuration: `round_width` 1 everywhere, routing
/// thresholds at zero, and approval disabled with a deny default.
///
/// Inside a [`Manifest`] a block or any of its fields may be omitted and takes
/// its value from this default. Decoded on its own, a `Policies` value must
/// spell out every block and field.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policies {
    /// Deliberation episode policy. `turn_budget` must be nonzero and at most
    /// `max_turns`; `round_width` nonzero and at most `revealed_width`, which
    /// is at most `max_width`; `dominance_cap` at most 100; `repetition_cap`,
    /// the quorum `threshold` and `window`, and the weights' `half_life`
    /// nonzero.
    pub episode: EpisodePolicy,
    /// Semantic routing policy. Its four probabilities are integer parts per
    /// million, at most `1000000`; `round_width` is nonzero and at most
    /// `max_width`; `choice_option_limit` is nonzero.
    pub routing: RoutingPolicy,
    /// Approval policy for typed actions; carried without extra bounds.
    pub approval: ApprovalPolicy,
    /// Private aside policy; when enabled, `max_members` and `max_messages`
    /// must be nonzero.
    pub aside: AsidePolicy,
    /// Cross-desk referral policy; when enabled, `max_hops` must be nonzero.
    pub referral: ReferralPolicy,
    /// Transcript digest policy; `input_limit` and `budget_chars` must be
    /// nonzero.
    pub digest: DigestPolicy,
    /// Message brevity policy; `message_chars` and `window` must be nonzero.
    pub brevity: BrevityPolicy,
    /// Expertise directory policy; `half_life` must be nonzero.
    pub directory: DirectoryPolicy,
    /// Peer exchange policy; when enabled, `contact_cap` and `round_cap` must
    /// be nonzero.
    pub exchange: ExchangePolicy,
    /// Task division policy; `round_width` must be nonzero and at most
    /// `max_width`.
    pub division: DivisionPolicy,
    /// Conducted episode turn walls; `child_turn_wall` must be nonzero and at
    /// most `turn_wall`, which is at most `max_turns`.
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
/// Host-neutral settings for a coordinator that schedules the hive's turns.
///
/// This crate only validates and carries them; a host maps them onto its own
/// coordinator (the `OpenHuman` adapter's `coordinator_options` copies them
/// field for field). Any omitted field takes its value from
/// [`CoordinatorSettings::default`].
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct CoordinatorSettings {
    /// Most turns the coordinator runs concurrently; nonzero and at most the
    /// constitution's `max_width`.
    pub round_width: usize,
    /// Turn walls for conducted episodes, bounded like
    /// [`Policies::conduct`].
    pub conduct_policy: ConductPolicy,
    /// Broadcasts allowed per assignment; `None` keeps the host driver's
    /// default, and `Some(0)` is rejected.
    pub broadcast_budget: Option<u32>,
    /// How many settled records the coordinator keeps.
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
/// Bounds on the records a coordinator retains; `None` means unbounded, and
/// is the value of any omitted field.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct RetentionSettings {
    /// Most recent finished episodes to keep.
    pub settled_episodes: Option<usize>,
    /// Most recent acknowledged deliveries to keep.
    pub delivered: Option<usize>,
    /// Most recent interruption records to keep.
    pub interrupted: Option<usize>,
    /// Most undelivered direct messages one agent may hold at once.
    pub pending_per_agent: Option<usize>,
}
