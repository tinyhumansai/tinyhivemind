//! One validated tool call and what its arguments mean.
use super::Kind;
use crate::{
    Result,
    policy::{ManagementRequest, SendRequest},
};
use serde_json::Value;
use tinyhivemind_hives::HiveInfo;

/// A hive tool call with validated arguments and a host-bound caller.
///
/// The host supplies the actor and session; no argument can.
#[derive(Clone, Debug)]
pub struct ToolCall {
    actor: String,
    session: Option<String>,
    kind: Kind,
    args: Value,
}
impl ToolCall {
    /// A call to `kind` by the seat the host bound it to.
    ///
    /// # Errors
    /// [`crate::Error::Arguments`] when `args` do not fit the declaration.
    pub fn new(
        actor: impl Into<String>,
        session: Option<String>,
        kind: Kind,
        args: Value,
    ) -> Result<Self> {
        kind.validate(&args)?;
        Ok(Self {
            actor: actor.into(),
            session,
            kind,
            args,
        })
    }
    pub(super) fn actor(&self) -> &str {
        &self.actor
    }
    pub(super) fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }
    pub(super) fn kind(&self) -> Kind {
        self.kind
    }
    pub(super) fn text(&self, name: &str) -> String {
        self.optional_text(name).unwrap_or_default()
    }
    pub(super) fn optional_text(&self, name: &str) -> Option<String> {
        self.args[name].as_str().map(str::to_owned)
    }
    pub(super) fn integer(&self, name: &str) -> Option<u64> {
        self.args[name].as_u64()
    }
    pub(super) fn strings(&self, name: &str) -> Vec<String> {
        self.args[name]
            .as_array()
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }
    /// The send this call makes; `None` for tools the send policy does not gate.
    pub(super) fn outbound(&self) -> Option<SendRequest> {
        Some(match self.kind {
            Kind::SendAgent => SendRequest::Agent {
                agent_id: self.text("agent_id"),
                body: self.text("body"),
            },
            Kind::SendHive => SendRequest::Hive {
                hive_id: self.text("hive_id"),
                body: self.text("body"),
                thread: self.integer("thread"),
                only_for: self.strings("only_for"),
            },
            Kind::Ask => SendRequest::Ask {
                episode_id: self.text("episode_id"),
                agents: self.strings("agents"),
                body: self.text("body"),
            },
            Kind::Broadcast => SendRequest::Broadcast {
                episode_id: self.text("episode_id"),
                body: self.text("body"),
            },
            _ => return None,
        })
    }
    /// The management request this call to a management tool makes.
    ///
    /// # Errors
    /// The memory contract does not decode.
    pub(super) fn management(&self) -> Result<ManagementRequest> {
        Ok(match self.kind {
            Kind::CreateHive => ManagementRequest::CreateHive(HiveInfo {
                hive_id: self.text("hive_id"),
                name: self.text("name"),
                description: self.optional_text("description"),
                members: self.strings("members"),
            }),
            Kind::CreateAgent => ManagementRequest::CreateAgent {
                template: self.text("template"),
                coordinator_id: self.optional_text("coordinator_id"),
                config: self.args["config"].clone(),
                memory: self
                    .args
                    .get("memory")
                    .filter(|v| !v.is_null())
                    .map(|v| serde_json::from_value(v.clone()))
                    .transpose()?,
            },
            Kind::JoinHive => ManagementRequest::JoinHive {
                hive_id: self.text("hive_id"),
                agent_id: self.text("agent_id"),
            },
            _ => ManagementRequest::LeaveHive {
                hive_id: self.text("hive_id"),
                agent_id: self.text("agent_id"),
            },
        })
    }
}
