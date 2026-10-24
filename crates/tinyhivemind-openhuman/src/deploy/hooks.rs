//! Captured membership context, turn gates and outbound send restrictions.
use super::{BuildOptions, Permissions, profile};
use crate::{HostedTurn, SendAuthorizer, SendRequest, TurnHooks, TurnOptions, TurnScope};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};
use tinyhivemind_core::runtime::{Recall, RecallMoment, RecallRequest};
use tinyhivemind_hives::TurnDisposition;
pub(super) struct Hooks {
    pub permissions: Arc<Permissions>,
    pub extra: Option<Arc<dyn TurnHooks>>,
    pub call: Option<openhuman_embed::budget::CallBudget>,
    memories: BTreeMap<String, crate::HiveMemoryStore>,
    recalled: Mutex<BTreeSet<(String, String, String)>>,
    /// Each seat's `stop` script hooks, by seat id.
    stops: BTreeMap<String, Arc<tinyhivemind_exec::HookRunner>>,
}
impl Hooks {
    #[cfg(test)]
    pub(super) fn new(permissions: Arc<Permissions>, options: &BuildOptions) -> Self {
        Self::with_memories(permissions, options, BTreeMap::new())
    }
    pub(super) fn with_memories(
        permissions: Arc<Permissions>,
        options: &BuildOptions,
        memories: BTreeMap<String, crate::HiveMemoryStore>,
    ) -> Self {
        Self {
            permissions,
            extra: options.hooks.clone(),
            call: options.call_budget,
            memories,
            recalled: Mutex::new(BTreeSet::new()),
            stops: BTreeMap::new(),
        }
    }

    /// Run each seat's `stop` script hooks on its finished replies.
    pub(super) fn with_stops(
        mut self,
        stops: BTreeMap<String, Arc<tinyhivemind_exec::HookRunner>>,
    ) -> Self {
        self.stops = stops;
        self
    }

    fn context_without_extra(&self, scope: &TurnScope) -> String {
        self.permissions
            .active
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(scope.agent_id.clone(), scope.clone());
        let config = self.permissions.config();
        if let Some(seat) = config.seats.iter().find(|seat| seat.id == scope.agent_id) {
            self.permissions
                .active
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(
                    seat.resolved_agent_id(&config.runtime.deployment_id),
                    scope.clone(),
                );
        }
        let mut text = String::new();
        if let Some(member) = Permissions::member(&config, scope) {
            for id in &member.context {
                if let Some(context) = config.contexts.get(id) {
                    text.push_str(context);
                    text.push('\n');
                }
            }
        }
        text
    }
}
impl TurnHooks for Hooks {
    fn context(&self, scope: &TurnScope) -> String {
        let mut text = self.context_without_extra(scope);
        if let Some(extra) = &self.extra {
            text.push_str(&extra.context(scope));
        }
        text
    }
    fn context_async<'a>(
        &'a self,
        scope: &'a TurnScope,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = String> + Send + 'a>> {
        Box::pin(async move {
            let mut text = self.context_without_extra(scope);
            if let Some(extra) = &self.extra {
                text.push_str(&extra.context_async(scope).await);
            }
            let Some(episode) = &scope.episode else {
                return text;
            };
            let Some(memory) = self.memories.get(&episode.hive_id) else {
                return text;
            };
            let conversation = scope.session_id.clone().unwrap_or_default();
            let key = (
                scope.agent_id.clone(),
                episode.hive_id.clone(),
                conversation.clone(),
            );
            // OpenHuman exposes no compaction callback to host hooks. Recall on
            // every turn instead, so the first turn after a compaction receives
            // a fresh synchronous hive briefing before the native session runs.
            let moment = if self
                .recalled
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(key)
            {
                RecallMoment::SessionStart
            } else {
                RecallMoment::Rejoin
            };
            let request = RecallRequest {
                seat: scope.agent_id.clone(),
                conversation,
                focus: Some(episode.brief.clone()),
                moment,
                budget_chars: 4_000,
            };
            if let Ok(notes) = memory.recall(&request).await
                && !notes.is_empty()
            {
                text.push_str("\nHive memory briefing:\n");
                for note in notes {
                    text.push_str(&note.heading);
                    text.push_str(":\n");
                    for line in note.lines {
                        text.push_str("- ");
                        text.push_str(&line);
                        text.push('\n');
                    }
                }
            }
            text
        })
    }
    fn prepare(&self, scope: &TurnScope) -> TurnOptions {
        self.extra
            .as_ref()
            .map_or_else(TurnOptions::default, |extra| extra.prepare(scope))
    }
    fn turn_timeout(&self, scope: &TurnScope) -> Option<Duration> {
        let config = self.permissions.config();
        let configured = config
            .seats
            .iter()
            .find(|seat| seat.id == scope.agent_id)
            .and_then(|seat| profile::limits(&config, seat).ok())
            .and_then(|limits| limits.timeout_ms)
            .map(Duration::from_millis);
        let extra = self
            .extra
            .as_ref()
            .and_then(|hooks| hooks.turn_timeout(scope));
        match (configured, extra) {
            (Some(configured), Some(extra)) => Some(configured.min(extra)),
            (configured, extra) => configured.or(extra),
        }
    }
    fn configure(
        &self,
        scope: &TurnScope,
        mut turn: openhuman_embed::Turn,
    ) -> openhuman_embed::Turn {
        let config = self.permissions.config();
        if let Some(seat) = config.seats.iter().find(|seat| seat.id == scope.agent_id)
            && let Ok(limits) = profile::limits(&config, seat)
            && let (Some(usd), Some(call)) = (limits.budget_usd, self.call)
        {
            turn = turn.budget(openhuman_embed::budget::ModelBudget {
                ledger: openhuman_embed::budget::Budget::new(
                    openhuman_embed::budget::SpendLimits {
                        tokens: None,
                        cost_micros: super::runtime::budget_micros(usd),
                    },
                ),
                call,
            });
        }
        let permissions = self.permissions.clone();
        let captured = scope.clone();
        turn = turn.can_use_tool(move |context| {
            let permissions = permissions.clone();
            let captured = captured.clone();
            Box::pin(async move {
                permissions
                    .decide(&captured.agent_id, Some(&captured), context)
                    .await
            })
        });
        match &self.extra {
            Some(extra) => extra.configure(scope, turn),
            None => turn,
        }
    }
    fn progress(&self, scope: &TurnScope) -> Option<crate::TurnProgressSink> {
        self.extra.as_ref().and_then(|extra| extra.progress(scope))
    }
    fn wrap_turn<'a>(&'a self, scope: &'a TurnScope, turn: HostedTurn<'a>) -> HostedTurn<'a> {
        let turn = match &self.extra {
            Some(extra) => extra.wrap_turn(scope, turn),
            None => turn,
        };
        match self.stops.get(&scope.agent_id).cloned() {
            // A stop hook observes the reply; it cannot start another turn.
            Some(stop) => Box::pin(async move {
                let outcome = turn.await;
                if let Ok(done) = &outcome {
                    stop.stop(&done.reply).await;
                }
                outcome
            }),
            None => turn,
        }
    }
    fn after_turn(
        &self,
        scope: &TurnScope,
        usage: Option<&openhuman_core::agent::tinyagents::host::LastTurnUsage>,
    ) -> crate::Result<TurnDisposition> {
        let config = self.permissions.config();
        let mut active = self
            .permissions
            .active
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        active.remove(&scope.agent_id);
        if let Some(seat) = config.seats.iter().find(|seat| seat.id == scope.agent_id) {
            active.remove(&seat.resolved_agent_id(&config.runtime.deployment_id));
        }
        drop(active);
        let extra = self
            .extra
            .as_ref()
            .map(|extra| extra.after_turn(scope, usage))
            .transpose()?
            .unwrap_or(TurnDisposition::Completed);
        if self
            .permissions
            .pending
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .values()
            .any(|pending| pending.scope.as_ref() == Some(scope))
        {
            Ok(TurnDisposition::Parked)
        } else {
            Ok(extra)
        }
    }
}
impl SendAuthorizer for Hooks {
    fn authorize(&self, actor: &str, request: &SendRequest) -> crate::Result<()> {
        self.authorize_in_session(actor, request, None)
    }
    fn authorize_in_session(
        &self,
        actor: &str,
        request: &SendRequest,
        session: Option<&str>,
    ) -> crate::Result<()> {
        let config = self.permissions.config();
        let seat = config
            .seats
            .iter()
            .find(|seat| {
                seat.id == actor || seat.resolved_agent_id(&config.runtime.deployment_id) == actor
            })
            .ok_or_else(|| crate::Error::SendDenied("unknown seat".into()))?;
        let active = self
            .permissions
            .active
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(actor)
            .cloned();
        if active
            .as_ref()
            .is_some_and(|scope| scope.session_id.as_deref() != session)
        {
            return Err(crate::Error::SendDenied(
                "send belongs to another active session".into(),
            ));
        }
        let member = active
            .as_ref()
            .and_then(|scope| Permissions::member(&config, scope));
        let (operation, destinations) = match request {
            SendRequest::Agent { agent_id, .. } => ("agent", vec![agent_id.as_str()]),
            SendRequest::Hive { hive_id, .. } => ("hive", vec![hive_id.as_str()]),
            SendRequest::Ask {
                episode_id, agents, ..
            } => {
                if active
                    .as_ref()
                    .and_then(|scope| scope.episode.as_ref())
                    .is_none_or(|episode| &episode.episode_id != episode_id)
                {
                    return Err(crate::Error::SendDenied("inactive episode".into()));
                }
                ("ask", agents.iter().map(String::as_str).collect())
            }
            SendRequest::Broadcast { episode_id, .. } => {
                let episode = active
                    .as_ref()
                    .and_then(|scope| scope.episode.as_ref())
                    .filter(|episode| &episode.episode_id == episode_id)
                    .ok_or_else(|| crate::Error::SendDenied("inactive episode".into()))?;
                ("broadcast", vec![episode.hive_id.as_str()])
            }
        };
        for policy in config
            .permission_layers(seat, member)
            .map_err(|_| crate::Error::SendDenied("invalid policy".into()))?
        {
            if policy
                .send_operations
                .as_ref()
                .is_some_and(|names| !names.iter().any(|name| name == operation))
                || policy.send_destinations.as_ref().is_some_and(|names| {
                    destinations
                        .iter()
                        .any(|id| !names.iter().any(|name| name == id))
                })
            {
                return Err(crate::Error::SendDenied("send permission denied".into()));
            }
        }
        Ok(())
    }
}
