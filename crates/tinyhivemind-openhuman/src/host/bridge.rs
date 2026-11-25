//! The adapter's policies and factory behind the host belt's ports.
//!
//! The host's ports return the host's error; the adapter's return its own,
//! which names `OpenHuman` types. Both directions carry the error unchanged.
use super::{AgentFactory, Management, OpenHumanHost, SendAuthorizer};
use crate::{Error, Result, error::into_host};
use std::sync::Arc;
use tinyhivemind_host::{
    policy::{self, CreateFuture, ManagementRequest, NewAgent, SendRequest},
    tools::{Belt, ToolCall},
};

/// The configured send policy.
pub(crate) struct SendPolicy(pub Arc<dyn SendAuthorizer>);
impl policy::SendAuthorizer for SendPolicy {
    fn authorize(&self, actor: &str, request: &SendRequest) -> tinyhivemind_host::Result<()> {
        self.0.authorize(actor, request).map_err(into_host)
    }
    fn authorize_in_session(
        &self,
        actor: &str,
        request: &SendRequest,
        session: Option<&str>,
    ) -> tinyhivemind_host::Result<()> {
        self.0
            .authorize_in_session(actor, request, session)
            .map_err(into_host)
    }
}
impl policy::ManagementAuthorizer for Management {
    fn authorize(&self, actor: &str, request: &ManagementRequest) -> tinyhivemind_host::Result<()> {
        self.authorizer.authorize(actor, request).map_err(into_host)
    }
}
/// Builds an agent with the host's factory and registers its handle.
struct Creator<'a> {
    host: &'a OpenHumanHost,
    factory: &'a dyn AgentFactory,
}
impl policy::AgentCreator for Creator<'_> {
    fn create(&self, agent: NewAgent) -> CreateFuture<'_> {
        Box::pin(async move { self.build(agent).await.map_err(into_host) })
    }
}
impl Creator<'_> {
    async fn build(&self, request: NewAgent) -> Result<String> {
        let NewAgent {
            template,
            coordinator_id,
            config,
            memory,
        } = request;
        if let Some(memory) = &memory {
            crate::language::validate_binding(memory)?;
        }
        let agent = self
            .factory
            .create_with_memory(template, config, memory.as_deref().cloned())
            .await?;
        if let Some(memory) = &memory {
            let installed = &agent.config().memory;
            if installed.agent_id.as_deref() != Some(memory.agent_id.as_str())
                || installed.root.as_deref() != Some(memory.root.as_str())
            {
                return Err(Error::UnboundSeat {
                    seat: agent.id().to_owned(),
                    reason: "factory did not install the requested memory binding".into(),
                });
            }
        }
        let id = coordinator_id.unwrap_or_else(|| agent.id().to_owned());
        self.host.register_as(agent, &id).await?;
        Ok(id)
    }
}
impl OpenHumanHost {
    /// Execute one validated hive tool call.
    pub(crate) async fn call(&self, call: &ToolCall) -> Result<serde_json::Value> {
        let creator = self.creator();
        Ok(self.belt(creator.as_ref()).call(call).await?)
    }
    /// Execute an authorized management request.
    /// # Errors
    /// Disabled management, host denial, factory, runtime or coordinator errors.
    pub async fn manage(
        &self,
        actor: &str,
        request: ManagementRequest,
    ) -> Result<serde_json::Value> {
        let creator = self.creator();
        Ok(self.belt(creator.as_ref()).manage(actor, request).await?)
    }
    fn creator(&self) -> Option<Creator<'_>> {
        self.inner.management.as_ref().map(|management| Creator {
            host: self,
            factory: management.factory.as_ref(),
        })
    }
    fn belt<'a>(&'a self, creator: Option<&'a Creator<'a>>) -> Belt<'a> {
        let mut belt = Belt::new(&self.inner.coordinator);
        if let Some(policy) = &self.inner.send_policy {
            belt = belt.with_send_policy(policy);
        }
        if let (Some(authorizer), Some(creator)) = (&self.inner.management, creator) {
            belt = belt.with_management(policy::Management {
                authorizer,
                creator,
            });
        }
        belt
    }
}
