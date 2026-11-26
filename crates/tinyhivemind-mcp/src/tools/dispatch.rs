//! One `tools/call`, from the model's arguments to the result it reads.
//!
//! In order: find the tool; for a state-changing one take `turn` and `call`
//! out of the arguments and refuse unless `turn` is the seat's running turn;
//! validate; then either hand a coordinator action with no hook to the belt
//! as an identified call, whose record the coordinator writes with the
//! effect, or go through the [`Ledger`]: answer a known call, judge the
//! host's send and management policy (hive tools) or the access gate
//! (programs), record the start, run the `pre_tool_use` hooks, run the
//! tool, run the `post_tool_use` hooks, record the finish. A call refused by
//! validation or policy before any hook runs records nothing.
use super::gate;
use super::ledger::{Identity, Known, Ledger, Outcome};
use super::{Catalog, Served, Target};
use crate::seat::{Bearer, SeatTools};
use crate::server::Services;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use tinyhivemind_exec::{HookRunner, Verdict};
use tinyhivemind_host::harness::ToolApproval;
use tinyhivemind_host::tools::{Kind, ToolCall};
use tinyhivemind_lang::SideEffects;

/// The note appended to a result whose stdout passed 1 MiB.
const TRUNCATED: &str = "\n[output truncated at 1 MiB]";

/// The model named a tool its seat is not served.
#[derive(Clone, Debug)]
pub(crate) struct UnknownTool(pub(crate) String);

/// A served seat: its bearer, its tools, and what runs them.
#[derive(Debug)]
pub(crate) struct SeatState {
    pub(crate) tools: SeatTools,
    pub(crate) bearer: Bearer,
    pub(crate) catalog: Catalog,
    hooks: HookRunner,
    /// Runs of each executable tool, for `THM_CALL_ID`.
    runs: Mutex<HashMap<String, u64>>,
}

impl SeatState {
    pub(crate) fn new(
        tools: SeatTools,
        bearer: Bearer,
        managed: bool,
        services: &Services,
    ) -> crate::Result<Self> {
        tools.check()?;
        let hooks = HookRunner::new(
            tools.hooks().to_vec(),
            std::sync::Arc::new(tools.context().clone()),
            services.secrets.clone(),
        );
        let catalog = Catalog::build(&tools, &hooks, managed)?;
        Ok(Self {
            tools,
            bearer,
            catalog,
            hooks,
            runs: Mutex::default(),
        })
    }

    fn id(&self) -> &str {
        self.tools.seat()
    }

    fn next_run(&self, tool: &str) -> String {
        let mut runs = self.runs.lock().unwrap_or_else(PoisonError::into_inner);
        let run = runs.entry(tool.to_owned()).or_default();
        *run += 1;
        format!("{tool}-{run}")
    }
}

/// Carry out one call of `name` with `arguments` as `seat`.
///
/// # Errors
/// [`UnknownTool`] when the seat is not served `name`; every other refusal is
/// an error [`Outcome`].
pub(crate) async fn call(
    services: &Services,
    ledger: &Ledger,
    seat: &SeatState,
    name: &str,
    arguments: Value,
) -> Result<Outcome, UnknownTool> {
    let tool = seat
        .catalog
        .find(name)
        .ok_or_else(|| UnknownTool(name.to_owned()))?;
    let mut arguments = match arguments {
        Value::Null => Value::Object(Map::new()),
        other => other,
    };
    let Some(object) = arguments.as_object_mut() else {
        return Ok(Outcome::refused("arguments must be an object"));
    };
    if !tool.changes_state {
        return Ok(run(services, seat, tool, &arguments, None).await);
    }
    let Some(turn) = take_text(object, "turn") else {
        return Ok(Outcome::refused(
            "missing argument turn: name your running turn",
        ));
    };
    let Some(call) = take_text(object, "call").filter(|call| !call.trim().is_empty()) else {
        return Ok(Outcome::refused(
            "missing argument call: name this action, uniquely within the turn",
        ));
    };
    let identity = Identity {
        seat: seat.id().to_owned(),
        turn,
        call,
    };
    if let Err(refusal) = admissible(services, seat, tool, &identity, &arguments) {
        return Ok(refusal);
    }
    let in_transaction = matches!(tool.target, Target::Hive(kind) if kind != Kind::CreateAgent);
    if in_transaction && !tool.hooked {
        return Ok(run(services, seat, tool, &arguments, Some(&identity)).await);
    }
    if let Some(answer) = answered(ledger.lookup(&identity, name, &arguments), &identity) {
        return Ok(answer);
    }
    if let Target::Hive(kind) = tool.target
        && let Err(refusal) = authorized(services, seat, kind, &arguments)
    {
        return Ok(refusal);
    }
    if let Target::Program(program) = &tool.target
        && program.side_effects != SideEffects::Read
    {
        let request = ToolApproval {
            seat: identity.seat.clone(),
            turn_id: identity.turn.clone(),
            call_id: identity.call.clone(),
            tool: name.to_owned(),
            arguments: arguments.clone(),
        };
        let approver = services.approver.as_ref();
        if let Err(refusal) = gate::admit(seat.tools.access(), approver, request).await {
            return Ok(Outcome::refused(refusal));
        }
    }
    if let Some(answer) = answered(ledger.start(&identity, name, &arguments), &identity) {
        return Ok(answer);
    }
    let outcome = if tool.hooked {
        match seat.hooks.pre_tool_use(name, &arguments).await {
            Verdict::Deny(message) => Outcome::refused(message),
            Verdict::Allow => {
                let outcome = run(services, seat, tool, &arguments, Some(&identity)).await;
                seat.hooks
                    .post_tool_use(name, &arguments, &outcome.text)
                    .await;
                outcome
            }
        }
    } else {
        run(services, seat, tool, &arguments, Some(&identity)).await
    };
    ledger.finish(&identity, outcome.clone());
    Ok(outcome)
}

/// Refuse a call from a turn that is not running, or with bad arguments.
fn admissible(
    services: &Services,
    seat: &SeatState,
    tool: &Served,
    identity: &Identity,
    arguments: &Value,
) -> Result<(), Outcome> {
    match services.coordinator.running_turn(seat.id()) {
        Ok(Some(running)) if running == identity.turn => {}
        Ok(_) => {
            let stale = tinyhivemind_hives::Error::StaleTurn {
                agent_id: identity.seat.clone(),
                turn_id: identity.turn.clone(),
            };
            return Err(Outcome::refused(stale.to_string()));
        }
        Err(error) => return Err(Outcome::refused(error.to_string())),
    }
    if let Target::Hive(kind) = tool.target {
        kind.validate(arguments)
            .map_err(|error| Outcome::refused(error.to_string()))?;
    }
    Ok(())
}

/// Refuse a hive call the host's send or management policy refuses, before
/// any hook runs. The belt checks again as it dispatches.
fn authorized(
    services: &Services,
    seat: &SeatState,
    kind: Kind,
    arguments: &Value,
) -> Result<(), Outcome> {
    let call = ToolCall::new(seat.id(), None, kind, arguments.clone())
        .map_err(|error| Outcome::refused(error.to_string()))?;
    services
        .belt()
        .authorize(&call)
        .map_err(|error| Outcome::refused(error.to_string()))
}

/// The answer the ledger already has for a call, if any.
fn answered(known: Known, identity: &Identity) -> Option<Outcome> {
    match known {
        Known::New => None,
        Known::Answered(outcome) => Some(outcome),
        Known::Running => Some(Outcome::refused(format!(
            "call {} has started and its outcome is not known; use a new call only to act again",
            identity.call
        ))),
        Known::Reused => Some(Outcome::refused(
            tinyhivemind_hives::Error::CallReused(identity.call.clone()).to_string(),
        )),
    }
}

/// Run `tool` itself, identified as `identity` when given.
async fn run(
    services: &Services,
    seat: &SeatState,
    tool: &Served,
    arguments: &Value,
    identity: Option<&Identity>,
) -> Outcome {
    match &tool.target {
        Target::Hive(kind) => {
            let call = match ToolCall::new(seat.id(), None, *kind, arguments.clone()) {
                Ok(call) => call,
                Err(error) => return Outcome::refused(error.to_string()),
            };
            let call = match identity {
                Some(identity) => call.in_turn(&identity.turn, &identity.call),
                None => call,
            };
            match services.belt().call(&call).await {
                Ok(value) => Outcome::success(value.to_string()),
                Err(error) => Outcome::refused(error.to_string()),
            }
        }
        Target::Program(program) => {
            let run_id = seat.next_run(&program.id);
            let context = seat.tools.context();
            let secrets = services.secrets.as_ref();
            match tinyhivemind_exec::run(program, context, secrets, arguments, &run_id).await {
                Ok(output) if output.truncated => Outcome::success(output.text + TRUNCATED),
                Ok(output) => Outcome::success(output.text),
                Err(error) => Outcome::refused(error.to_string()),
            }
        }
    }
}

fn take_text(object: &mut Map<String, Value>, name: &str) -> Option<String> {
    match object.remove(name) {
        Some(Value::String(text)) => Some(text),
        _ => None,
    }
}
