//! Bind seats to host-neutral memory namespaces and expose core memory ports.
//!
//! [`HiveMemory`] supports hosts that bind every registered seat to one hive
//! root. Configured deployments use a stable seat id under a deployment root
//! for `OpenHuman`'s own conversation logging, then use one [`HiveMemoryStore`]
//! per hive for synchronous, hive-scoped coordinator recall. Those stores and
//! the runtime share one engine `Arc`; configured deployments disable
//! `OpenHuman`'s process-wide recall prefetch because it has no active-hive key.
mod convert;
mod store;
pub use store::HiveMemoryStore;

use crate::{Error, Result};
use openhuman_embed::{Agent, AgentSpec, MemoryBinding, RuntimeConfig};
use std::num::NonZeroU32;

/// Longest memory agent id, and longest hive id in a derived root: one
/// `TinyMemory` namespace segment.
const MAX_ID_LEN: usize = 128;
const MAX_AGENT_ID_LEN: usize = 64;

/// Where the seats of one hive keep their shared memory.
///
/// Build it with [`HiveMemory::for_hive`] (root `team:<hive-id>`) or
/// [`HiveMemory::with_root`], then hand it to
/// [`OpenHumanHost::with_hive_memory`](crate::OpenHumanHost::with_hive_memory).
/// Each seat's memory agent id is its seat id unchanged
/// ([`HiveMemory::agent_id`]); the root already keeps it apart from the same
/// seat id in another hive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HiveMemory {
    root: String,
    recall_budget_tokens: Option<NonZeroU32>,
}

impl HiveMemory {
    /// Memory shared under `team:<hive_id>`.
    ///
    /// # Errors
    /// [`Error::InvalidMemoryRoot`] when `hive_id` is not 1 to 128 characters
    /// of `A-Z`, `a-z`, `0-9`, `_` or `-`. Ids are refused rather than
    /// rewritten so the root a host names is the root the engine stores.
    pub fn for_hive(hive_id: &str) -> Result<Self> {
        let root = format!("team:{hive_id}");
        if let Err(reason) = check_segment(hive_id) {
            return Err(Error::InvalidMemoryRoot { root, reason });
        }
        Self::with_root(root)
    }

    /// Memory shared under an explicit layout root, such as `team:acme` or
    /// `project:q4/team:ops`.
    ///
    /// # Errors
    /// [`Error::InvalidMemoryRoot`] when `OpenHuman` would not accept `root`
    /// as a memory root, or when it names the runtime's default root (blank
    /// or `root`), which isolates nothing.
    pub fn with_root(root: impl Into<String>) -> Result<Self> {
        let root = root.into().trim().to_owned();
        if root.is_empty() || root == "root" {
            return Err(Error::InvalidMemoryRoot {
                root,
                reason: "the default root shares memory with every unbound agent".into(),
            });
        }
        tinymemory_tools::validate_root(&root).map_err(|reason| Error::InvalidMemoryRoot {
            root: root.clone(),
            reason,
        })?;
        Ok(Self {
            root,
            recall_budget_tokens: None,
        })
    }

    /// Size the per-turn recall pack every seat receives, in tokens.
    ///
    /// `OpenHuman` reads it from `[memory.recall] budget_tokens` of each
    /// agent's config. A seat's config derives from its runtime's, so apply
    /// it with [`HiveMemory::configure`] on the config the runtime is built
    /// from; registration then refuses a seat built without it.
    #[must_use]
    pub fn recall_budget_tokens(mut self, tokens: NonZeroU32) -> Self {
        self.recall_budget_tokens = Some(tokens);
        self
    }

    /// The shared layout root.
    #[must_use]
    pub fn root(&self) -> &str {
        &self.root
    }

    /// The configured recall budget, when one was set.
    #[must_use]
    pub fn budget(&self) -> Option<NonZeroU32> {
        self.recall_budget_tokens
    }

    /// The memory agent id of `seat`: the seat id itself.
    ///
    /// # Errors
    /// [`Error::InvalidMemoryAgentId`] when `seat` does not match `OpenHuman`'s
    /// lowercase, 64-character agent-id syntax.
    pub fn agent_id(&self, seat: &str) -> Result<String> {
        check_id(seat).map_err(|reason| Error::InvalidMemoryAgentId {
            agent_id: seat.to_owned(),
            reason,
        })?;
        Ok(seat.to_owned())
    }

    /// The `OpenHuman` memory binding of `seat`: its memory agent id under
    /// this hive's root.
    ///
    /// # Errors
    /// [`Error::InvalidMemoryAgentId`] for an unusable seat id.
    pub fn binding(&self, seat: &str) -> Result<MemoryBinding> {
        Ok(MemoryBinding::new(self.agent_id(seat)?).root(self.root.clone()))
    }

    /// Bind the seat `spec` describes to this hive's memory.
    ///
    /// # Errors
    /// [`Error::InvalidMemoryAgentId`] for an unusable seat id.
    pub fn bind(&self, spec: AgentSpec) -> Result<AgentSpec> {
        let binding = self.binding(spec.id())?;
        Ok(spec.memory(binding))
    }

    /// Apply the hive-wide recall settings to a runtime config. Seat
    /// identities are per agent and come from [`HiveMemory::bind`].
    pub fn configure(&self, config: &mut RuntimeConfig) {
        config.memory.layout = openhuman_core::config::MemoryLayoutMode::Legacy;
        if let Some(tokens) = self.recall_budget_tokens {
            config.memory.recall.budget_tokens = tokens.get();
        }
    }

    /// Refuse a seat whose built config is not bound to this hive's memory.
    pub(crate) fn check(&self, agent: &Agent) -> Result<()> {
        let memory = &agent.config().memory;
        let expected = self.agent_id(agent.id())?;
        let unbound = |reason: String| Error::UnboundSeat {
            seat: agent.id().to_owned(),
            reason,
        };
        if memory.agent_id.as_deref() != Some(expected.as_str()) {
            return Err(unbound(format!(
                "memory agent id is {:?}, expected {expected:?}",
                memory.agent_id
            )));
        }
        if memory.root.as_deref() != Some(self.root.as_str()) {
            return Err(unbound(format!(
                "memory root is {:?}, expected {:?}",
                memory.root, self.root
            )));
        }
        if let Some(tokens) = self.recall_budget_tokens
            && memory.recall.budget_tokens != tokens.get()
        {
            return Err(unbound(format!(
                "recall budget is {} tokens, expected {tokens}",
                memory.recall.budget_tokens
            )));
        }
        Ok(())
    }
}

/// One `TinyMemory` namespace segment id, kept verbatim by the engine.
fn check_id(id: &str) -> std::result::Result<(), String> {
    let bytes = id.as_bytes();
    let valid = !bytes.is_empty()
        && bytes.len() <= MAX_AGENT_ID_LEN
        && (bytes[0].is_ascii_lowercase() || bytes[0].is_ascii_digit())
        && id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-'
        });
    if valid {
        Ok(())
    } else {
        Err(format!(
            "must match OpenHuman's 1 to {MAX_AGENT_ID_LEN} lowercase letters, digits, `_` or `-` syntax"
        ))
    }
}

fn check_segment(id: &str) -> std::result::Result<(), String> {
    if !id.is_empty()
        && id.len() <= MAX_ID_LEN
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        Ok(())
    } else {
        Err(format!(
            "must be 1 to {MAX_ID_LEN} characters of A-Z, a-z, 0-9, `_` or `-`"
        ))
    }
}

#[cfg(test)]
mod test;
