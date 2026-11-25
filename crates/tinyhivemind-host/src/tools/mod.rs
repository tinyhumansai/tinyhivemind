//! The hive tools and the one belt that executes them.
//!
//! A harness adapter serves [`Kind`] to a model, builds a [`ToolCall`] from what
//! the model sent and who the host bound the tool to, and hands it to a
//! [`Belt`]. The belt checks the host's policies, then acts on the coordinator.
mod call;
mod types;
use crate::{
    Error, Result,
    policy::{Management, ManagementRequest, NewAgent, SendAuthorizer},
};
pub use call::ToolCall;
use serde_json::{Value, json};
use tinyhivemind_hives::{Coordinator, Destination, EpisodeAction, SendMessage, TurnCall};
pub use types::Kind;

/// Executes hive tool calls against a coordinator under the host's policies.
#[derive(Clone, Copy)]
pub struct Belt<'a> {
    coordinator: &'a Coordinator,
    send_policy: Option<&'a dyn SendAuthorizer>,
    management: Option<Management<'a>>,
}
impl std::fmt::Debug for Belt<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Belt")
            .field("send_policy", &self.send_policy.is_some())
            .field("management", &self.management.is_some())
            .finish_non_exhaustive()
    }
}
impl<'a> Belt<'a> {
    /// A belt over `coordinator` that admits every send and manages nothing.
    #[must_use]
    pub fn new(coordinator: &'a Coordinator) -> Self {
        Self {
            coordinator,
            send_policy: None,
            management: None,
        }
    }
    /// Gate the outbound tools behind `policy`.
    #[must_use]
    pub fn with_send_policy(mut self, policy: &'a dyn SendAuthorizer) -> Self {
        self.send_policy = Some(policy);
        self
    }
    /// Enable the management tools.
    #[must_use]
    pub fn with_management(mut self, management: Management<'a>) -> Self {
        self.management = Some(management);
        self
    }
    /// Execute one validated call as its bound actor.
    ///
    /// # Errors
    /// The send policy's refusal, a management failure, or the coordinator's
    /// refusal of the operation.
    pub async fn call(&self, call: &ToolCall) -> Result<Value> {
        let coordinator = self.coordinator;
        let actor = call.actor();
        if let (Some(policy), Some(request)) = (self.send_policy, call.outbound()) {
            policy.authorize_in_session(actor, &request, call.session())?;
        }
        match call.kind() {
            Kind::ListHives => Ok(serde_json::to_value(
                coordinator
                    .list_hives()?
                    .into_iter()
                    .filter(|hive| hive.members.iter().any(|member| member == actor))
                    .collect::<Vec<_>>(),
            )?),
            Kind::ListAgents => Ok(serde_json::to_value(coordinator.list_agents()?)?),
            Kind::Read => {
                let rows = if let Some(peer) = call.optional_text("agent_id") {
                    coordinator.read_direct(actor, &peer, call.integer("after"))?
                } else {
                    coordinator.read_hive(
                        actor,
                        &call.text("hive_id"),
                        call.integer("after"),
                        call.integer("thread"),
                    )?
                };
                Ok(serde_json::to_value(rows)?)
            }
            Kind::SendHive => {
                self.send(call, Destination::Hive(call.text("hive_id")))
                    .await
            }
            Kind::SendAgent => {
                self.send(call, Destination::Agent(call.text("agent_id")))
                    .await
            }
            Kind::Post => {
                let body = call.text("body");
                self.submit(call, EpisodeAction::Post { body }).await
            }
            Kind::Ask => {
                let body = call.text("body");
                let agents = call.strings("agents");
                self.submit(call, EpisodeAction::Ask { body, agents }).await
            }
            Kind::Broadcast => {
                let body = call.text("body");
                self.submit(call, EpisodeAction::Broadcast { body }).await
            }
            Kind::Complete => {
                let body = call.text("body");
                self.submit(call, EpisodeAction::Complete { body }).await
            }
            Kind::CreateHive | Kind::CreateAgent | Kind::JoinHive | Kind::LeaveHive => {
                let turn = call.turn_call();
                self.manage_as(actor, call.management()?, turn.as_ref())
                    .await
            }
        }
    }
    /// Execute an authorized management request.
    ///
    /// # Errors
    /// Disabled management, host denial, agent creation or coordinator errors.
    pub async fn manage(&self, actor: &str, request: ManagementRequest) -> Result<Value> {
        self.manage_as(actor, request, None).await
    }
    /// [`Self::manage`], through the coordinator's identified calls when `turn`
    /// is given. Creating an agent is the creator's, outside them.
    async fn manage_as(
        &self,
        actor: &str,
        request: ManagementRequest,
        turn: Option<&TurnCall>,
    ) -> Result<Value> {
        let management = self.management.ok_or(Error::ManagementDisabled)?;
        management.authorizer.authorize(actor, &request)?;
        let coordinator = self.coordinator;
        match request {
            ManagementRequest::CreateHive(hive) => {
                match turn {
                    Some(turn) => coordinator.create_hive_in_turn(turn, hive.clone()).await?,
                    None => coordinator.create_hive(hive.clone()).await?,
                }
                Ok(serde_json::to_value(hive)?)
            }
            ManagementRequest::CreateAgent {
                template,
                coordinator_id,
                config,
                memory,
            } => {
                let agent = NewAgent {
                    template,
                    coordinator_id,
                    config,
                    memory,
                };
                let agent_id = management.creator.create(agent).await?;
                Ok(json!({"agent_id":agent_id}))
            }
            ManagementRequest::JoinHive { hive_id, agent_id } => {
                match turn {
                    Some(turn) => {
                        coordinator
                            .join_hive_in_turn(turn, &hive_id, &agent_id)
                            .await?;
                    }
                    None => coordinator.join_hive(&hive_id, &agent_id).await?,
                }
                Ok(json!({"joined":true}))
            }
            ManagementRequest::LeaveHive { hive_id, agent_id } => {
                match turn {
                    Some(turn) => {
                        coordinator
                            .leave_hive_in_turn(turn, &hive_id, &agent_id)
                            .await?;
                    }
                    None => coordinator.leave_hive(&hive_id, &agent_id).await?,
                }
                Ok(json!({"left":true}))
            }
        }
    }
    async fn send(&self, call: &ToolCall, destination: Destination) -> Result<Value> {
        let message = SendMessage {
            message_id: call.text("message_id"),
            sender: call.actor().to_owned(),
            destination,
            body: call.text("body"),
            thread: call.integer("thread"),
            only_for: call.strings("only_for"),
            starters: Vec::new(),
        };
        let receipt = match call.turn_call() {
            Some(turn) => self.coordinator.send_in_turn(&turn, message).await?,
            None => self.coordinator.send(message).await?,
        };
        Ok(serde_json::to_value(receipt)?)
    }
    async fn submit(&self, call: &ToolCall, action: EpisodeAction) -> Result<Value> {
        let episode = call.text("episode_id");
        match call.turn_call() {
            Some(turn) => {
                self.coordinator
                    .submit_action_in_turn(&turn, &episode, action)
                    .await?;
            }
            None => {
                self.coordinator
                    .submit_action(call.actor(), &episode, action)
                    .await?;
            }
        }
        Ok(json!({"accepted":true}))
    }
}

#[cfg(test)]
mod test;
