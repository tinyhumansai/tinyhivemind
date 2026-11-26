//! What an agent asked to do, as the host's policies see it.
use tinyhivemind_hives::HiveInfo;
use tinyhivemind_lang::LoweredMemoryBinding;
/// Outbound operation offered to the host [`SendAuthorizer`](super::SendAuthorizer).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum SendRequest {
    /// Direct message to a registered agent.
    Agent {
        /// Recipient agent identity.
        agent_id: String,
        /// Message text.
        body: String,
    },
    /// Message to a hive the actor belongs to.
    Hive {
        /// Hive identity.
        hive_id: String,
        /// Message text.
        body: String,
        /// Existing conversation root, when replying in a thread.
        thread: Option<u64>,
        /// Private readers; empty means every member.
        only_for: Vec<String>,
    },
    /// Question to peers in the actor's active episode.
    Ask {
        /// Episode the question is asked in.
        episode_id: String,
        /// Asked peers.
        agents: Vec<String>,
        /// Question text.
        body: String,
    },
    /// Work routed across the actor's active episode.
    Broadcast {
        /// Episode the work is broadcast in.
        episode_id: String,
        /// Work text.
        body: String,
    },
}
/// Management operation offered to the host authorizer.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq)]
pub enum ManagementRequest {
    /// Create a hive using known agents.
    CreateHive(HiveInfo),
    /// Create a configured agent through a host-defined template.
    CreateAgent {
        /// Template reference.
        template: String,
        /// Optional coordinator identity, separate from factory-owned settings.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        coordinator_id: Option<String>,
        /// Nonsecret host-validated settings.
        config: serde_json::Value,
        /// Portable memory contract, applied by the factory before registration.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        memory: Option<Box<LoweredMemoryBinding>>,
    },
    /// Join a registered agent to a hive.
    JoinHive {
        /// Hive identity.
        hive_id: String,
        /// Agent identity.
        agent_id: String,
    },
    /// Leave a hive without deleting its history.
    LeaveHive {
        /// Hive identity.
        hive_id: String,
        /// Agent identity.
        agent_id: String,
    },
}
/// The agent an authorized [`ManagementRequest::CreateAgent`] describes.
#[derive(Clone, Debug, PartialEq)]
pub struct NewAgent {
    /// Template reference.
    pub template: String,
    /// Coordinator identity, when the request chose one.
    pub coordinator_id: Option<String>,
    /// Nonsecret host-validated settings.
    pub config: serde_json::Value,
    /// Portable memory contract, applied before registration.
    pub memory: Option<Box<LoweredMemoryBinding>>,
}
