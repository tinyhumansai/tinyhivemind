//! Stable native tools with bound attribution and weak service references.
use crate::host::{Activation, Inner};
use crate::{Error, OpenHumanHost};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Weak;
use tinyhivemind_host::tools::{Kind, ToolCall};
use tinytools::{PermissionLevel, Tool, ToolResult};
#[cfg(test)]
pub(crate) fn belt(
    actor: &str,
    host: &Weak<Inner>,
    activation: &std::sync::Arc<Activation>,
    managed: bool,
) -> Vec<Box<dyn Tool>> {
    belt_in_session(actor, host, activation, managed, None)
}
pub(crate) fn belt_in_session(
    actor: &str,
    host: &Weak<Inner>,
    activation: &std::sync::Arc<Activation>,
    managed: bool,
    session: Option<&str>,
) -> Vec<Box<dyn Tool>> {
    Kind::all(managed)
        .into_iter()
        .map(|kind| {
            Box::new(HiveTool {
                actor: actor.into(),
                session: session.map(str::to_owned),
                host: host.clone(),
                kind,
                activation: activation.clone(),
            }) as Box<dyn Tool>
        })
        .collect()
}
struct HiveTool {
    actor: String,
    session: Option<String>,
    host: Weak<Inner>,
    kind: Kind,
    activation: std::sync::Arc<Activation>,
}
#[async_trait]
impl Tool for HiveTool {
    fn name(&self) -> &str {
        self.kind.name()
    }
    fn description(&self) -> &str {
        self.kind.description()
    }
    fn parameters_schema(&self) -> Value {
        self.kind.schema()
    }
    fn permission_level(&self) -> PermissionLevel {
        match self.kind {
            Kind::CreateHive | Kind::CreateAgent | Kind::JoinHive | Kind::LeaveHive => {
                PermissionLevel::Write
            }
            // Protocol state is internal to the shared conversation. Actor,
            // episode and send policy checks still authorize each operation.
            _ => PermissionLevel::ReadOnly,
        }
    }
    fn policy(&self) -> tinytools::ToolPolicy {
        if self.permission_level() == PermissionLevel::ReadOnly {
            tinytools::ToolPolicy::classified().with_side_effects(tinytools::ToolSideEffects {
                read_only: true,
                ..Default::default()
            })
        } else {
            tinytools::ToolPolicy::default()
        }
    }
    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let result = async {
            let call = ToolCall::new(self.actor.as_str(), self.session.clone(), self.kind, args)?;
            let host = OpenHumanHost {
                inner: self.host.upgrade().ok_or(Error::Unavailable)?,
            };
            if !self.activation.is_ready() {
                return Err(Error::RegistrationPending);
            }
            host.call(&call).await
        }
        .await;
        Ok(match result {
            Ok(value) => ToolResult::success(serde_json::to_string(&value)?),
            Err(error) => ToolResult::error(error.to_string()),
        })
    }
}

#[cfg(test)]
mod direct_test;
#[cfg(test)]
mod policy_test;
#[cfg(test)]
mod test;
