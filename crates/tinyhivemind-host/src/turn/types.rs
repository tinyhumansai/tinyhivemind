//! What a turn is about and what a host may choose for it.
use std::path::PathBuf;
use tinyhivemind_hives::{Destination, EpisodeContext, TurnRequest};
/// What one turn is about, derived from its [`TurnRequest`].
///
/// Every hook receives it, so a host can route live progress, attribute an
/// approval, or file a card per hive, episode and thread rather than per agent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TurnScope {
    /// Durable reservation identity from the authoritative coordinator request.
    pub turn_id: String,
    /// Native session authenticated by the runner before any hooks execute.
    pub session_id: Option<String>,
    /// Durable trusted scheduled provenance; cannot be supplied by message text.
    pub scheduled_job_id: Option<String>,
    /// The agent taking the turn.
    pub agent_id: String,
    /// Active conductor assignment, absent for a direct message.
    pub episode: Option<EpisodeContext>,
    /// Identities of the messages delivered in this turn, in order.
    pub message_ids: Vec<String>,
    /// Distinct senders of those messages, in first-seen order.
    pub senders: Vec<String>,
    /// The hive for an episode turn; otherwise the delivered message's
    /// destination, which is the agent itself.
    pub destination: Destination,
    /// Conversation root the turn answers, when it answers one.
    pub thread: Option<u64>,
}
impl TurnScope {
    /// Derive the scope of `request`.
    #[must_use]
    pub fn from_request(request: &TurnRequest) -> Self {
        let first = request.messages.first();
        let mut senders: Vec<String> = Vec::new();
        for message in &request.messages {
            if !senders.contains(&message.sender) {
                senders.push(message.sender.clone());
            }
        }
        let destination = match (&request.episode, first) {
            (Some(episode), _) => Destination::Hive(episode.hive_id.clone()),
            (None, Some(message)) => message.destination.clone(),
            (None, None) => Destination::Agent(request.agent_id.clone()),
        };
        Self {
            turn_id: request.turn_id.clone(),
            scheduled_job_id: request.scheduled_job_id.clone(),
            session_id: request.session_id.clone(),
            agent_id: request.agent_id.clone(),
            episode: request.episode.clone(),
            message_ids: request
                .messages
                .iter()
                .map(|message| message.message_id.clone())
                .collect(),
            senders,
            destination,
            thread: request
                .episode
                .as_ref()
                .map_or_else(|| first.and_then(|message| message.thread), |e| e.thread),
        }
    }
}
/// Per-turn settings a host chooses before the turn is built.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TurnOptions {
    /// Working directory for the agent's filesystem and shell tools this
    /// turn; `None` keeps the agent's own.
    pub cwd: Option<PathBuf>,
}
