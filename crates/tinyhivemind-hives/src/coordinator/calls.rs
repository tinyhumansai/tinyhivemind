//! Identified calls: an action taken from a running turn, recorded with it.
//!
//! Each `*_in_turn` entry point checks the call's turn, answers a known call
//! from its record, or acts and records the call, all inside one transaction:
//! a commit holds both the effect and its record, or neither. The record lives
//! on the [`crate::RunningTurn`] and goes when the turn ends. A coordinator
//! restart interrupts every running turn, so a retry after one is refused as
//! stale rather than answered.
use super::{
    Coordinator, EpisodeAction, HiveInfo, Receipt, SendMessage, TurnCall, create_hive_state,
    identifier, join_hive_state, leave_hive_state,
    messaging::{accept_state, admissible},
    stage_action,
};
use crate::{CallRecord, Error, Result, StoredState};
use serde::{Serialize, de::DeserializeOwned};

impl Coordinator {
    /// The id of `agent_id`'s running turn, if it has one.
    /// # Errors
    /// Returns a poisoned shared lock error.
    pub fn running_turn(&self, agent_id: &str) -> Result<Option<String>> {
        Ok(self
            .lock()?
            .durable
            .running
            .get(agent_id)
            .map(|running| running.request.turn_id.clone()))
    }
    /// Whether `call` is recorded on its agent's running turn with the same
    /// tool and arguments, so that an `*_in_turn` retry of it would be
    /// answered from the record rather than act.
    /// # Errors
    /// Returns a poisoned shared lock error.
    pub fn call_recorded(&self, call: &TurnCall) -> Result<bool> {
        Ok(self
            .lock()?
            .durable
            .running
            .get(&call.agent_id)
            .filter(|running| running.request.turn_id == call.turn_id)
            .and_then(|running| running.calls.get(&call.call))
            .is_some_and(|record| record.tool == call.tool && record.arguments == call.arguments))
    }
    /// [`Self::submit_action`] as an identified call of the caller's running turn.
    /// # Errors
    /// [`Error::StaleTurn`], [`Error::CallReused`], an empty identity, or the
    /// errors of [`Self::submit_action`].
    pub async fn submit_action_in_turn(
        &self,
        call: &TurnCall,
        episode_id: &str,
        action: EpisodeAction,
    ) -> Result<()> {
        self.in_turn(call, |state| {
            stage_action(state, &call.agent_id, episode_id, &action)
        })
        .await
    }
    /// [`Self::send`] as an identified call; the sender must be the caller.
    ///
    /// The send's `message_id` is still checked, after `call`: two calls that
    /// send one `message_id` with one payload are one message.
    /// # Errors
    /// [`Error::StaleTurn`], [`Error::CallReused`], an empty identity, a
    /// sender other than the caller, or the errors of [`Self::send`].
    pub async fn send_in_turn(&self, call: &TurnCall, request: SendMessage) -> Result<Receipt> {
        if request.sender != call.agent_id {
            return Err(Error::InvalidIdentifier("sender"));
        }
        admissible(&request)?;
        let retention = self.inner.options.retention;
        self.in_turn(call, |state| accept_state(state, &request, None, retention))
            .await
    }
    /// [`Self::create_hive`] as an identified call.
    /// # Errors
    /// [`Error::StaleTurn`], [`Error::CallReused`], an empty identity, or the
    /// errors of [`Self::create_hive`].
    pub async fn create_hive_in_turn(&self, call: &TurnCall, hive: HiveInfo) -> Result<()> {
        identifier(&hive.hive_id, "hive id")?;
        identifier(&hive.name, "hive name")?;
        self.in_turn(call, |state| create_hive_state(state, &hive))
            .await
    }
    /// [`Self::join_hive`] as an identified call.
    /// # Errors
    /// [`Error::StaleTurn`], [`Error::CallReused`], an empty identity, or the
    /// errors of [`Self::join_hive`].
    pub async fn join_hive_in_turn(
        &self,
        call: &TurnCall,
        hive_id: &str,
        agent_id: &str,
    ) -> Result<()> {
        self.in_turn(call, |state| join_hive_state(state, hive_id, agent_id))
            .await
    }
    /// [`Self::leave_hive`] as an identified call.
    /// # Errors
    /// [`Error::StaleTurn`], [`Error::CallReused`], an empty identity, or the
    /// errors of [`Self::leave_hive`].
    pub async fn leave_hive_in_turn(
        &self,
        call: &TurnCall,
        hive_id: &str,
        agent_id: &str,
    ) -> Result<()> {
        self.in_turn(call, |state| leave_hive_state(state, hive_id, agent_id))
            .await
    }
    /// Check `call`'s turn, then answer it from its record or run `act` and
    /// record it, in one transaction. A recorded answer commits nothing.
    async fn in_turn<T: Serialize + DeserializeOwned>(
        &self,
        call: &TurnCall,
        mut act: impl FnMut(&mut StoredState) -> Result<T>,
    ) -> Result<T> {
        identifier(&call.agent_id, "agent id")?;
        identifier(&call.turn_id, "turn id")?;
        identifier(&call.call, "call")?;
        let stale = || Error::StaleTurn {
            agent_id: call.agent_id.clone(),
            turn_id: call.turn_id.clone(),
        };
        let gate = self.inner.writer.lock().await;
        let value = self
            .transact(&gate, |state| {
                let running = state
                    .running
                    .get(&call.agent_id)
                    .filter(|running| running.request.turn_id == call.turn_id)
                    .ok_or_else(stale)?;
                if let Some(record) = running.calls.get(&call.call) {
                    if record.tool != call.tool || record.arguments != call.arguments {
                        return Err(Error::CallReused(call.call.clone()));
                    }
                    return Ok((false, serde_json::from_value(record.result.clone())?));
                }
                let value = act(state)?;
                let record = CallRecord {
                    tool: call.tool.clone(),
                    arguments: call.arguments.clone(),
                    result: serde_json::to_value(&value)?,
                };
                state
                    .running
                    .get_mut(&call.agent_id)
                    .ok_or_else(stale)?
                    .calls
                    .insert(call.call.clone(), record);
                Ok((true, value))
            })
            .await?;
        self.inner.notify.notify_one();
        Ok(value)
    }
}
