//! The in-memory started and finished records of calls that act outside the
//! coordinator's transaction: executable tools, `create_agent`, and every
//! call a hook matches.
//!
//! Each seat keeps the records of its running turn only. A call reaches the
//! ledger after its turn was checked to be running, so a call naming a newer
//! turn means the older one has ended and its records are dropped. The
//! records are this process's alone: at most once holds within one server
//! process.
use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};
use serde_json::Value;

/// What a call returned to the model.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Outcome {
    /// The tool result's text.
    pub(crate) text: String,
    /// Whether it is an error result.
    pub(crate) is_error: bool,
}

impl Outcome {
    /// A result.
    pub(crate) fn success(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_error: false,
        }
    }

    /// A refusal or failure the model reads.
    pub(crate) fn refused(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            is_error: true,
        }
    }
}

/// (seat, turn, call).
#[derive(Clone, Debug)]
pub(crate) struct Identity {
    pub(crate) seat: String,
    pub(crate) turn: String,
    pub(crate) call: String,
}

/// What the ledger knows of a call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Known {
    /// Nothing: it may act.
    New,
    /// It finished with this outcome.
    Answered(Outcome),
    /// It started and has not finished, or never will.
    Running,
    /// Its `call` was used for another tool or other arguments.
    Reused,
}

#[derive(Debug)]
struct Entry {
    tool: String,
    arguments: Value,
    outcome: Option<Outcome>,
}

#[derive(Debug, Default)]
struct Turn {
    turn: String,
    calls: HashMap<String, Entry>,
}

/// Every seat's records for its running turn.
#[derive(Debug, Default)]
pub(crate) struct Ledger(Mutex<HashMap<String, Turn>>);

impl Ledger {
    /// What is known of `identity` as a call of `tool` with `arguments`.
    pub(crate) fn lookup(&self, identity: &Identity, tool: &str, arguments: &Value) -> Known {
        self.with_turn(identity, |calls| known(calls, &identity.call, tool, arguments))
    }

    /// Record `identity` as started, unless it is known already.
    pub(crate) fn start(&self, identity: &Identity, tool: &str, arguments: &Value) -> Known {
        self.with_turn(identity, |calls| {
            let known = known(calls, &identity.call, tool, arguments);
            if known == Known::New {
                calls.insert(
                    identity.call.clone(),
                    Entry {
                        tool: tool.into(),
                        arguments: arguments.clone(),
                        outcome: None,
                    },
                );
            }
            known
        })
    }

    /// Record what a started call returned. A call whose turn has since been
    /// replaced has no record left to finish.
    pub(crate) fn finish(&self, identity: &Identity, outcome: Outcome) {
        let mut seats = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(turn) = seats
            .get_mut(&identity.seat)
            .filter(|turn| turn.turn == identity.turn)
            && let Some(entry) = turn.calls.get_mut(&identity.call)
        {
            entry.outcome = Some(outcome);
        }
    }

    fn with_turn<T>(&self, identity: &Identity, act: impl FnOnce(&mut HashMap<String, Entry>) -> T) -> T {
        let mut seats = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let turn = seats.entry(identity.seat.clone()).or_default();
        if turn.turn != identity.turn {
            *turn = Turn {
                turn: identity.turn.clone(),
                calls: HashMap::new(),
            };
        }
        act(&mut turn.calls)
    }
}

fn known(calls: &HashMap<String, Entry>, call: &str, tool: &str, arguments: &Value) -> Known {
    match calls.get(call) {
        None => Known::New,
        Some(entry) if entry.tool != tool || entry.arguments != *arguments => Known::Reused,
        Some(Entry {
            outcome: Some(outcome),
            ..
        }) => Known::Answered(outcome.clone()),
        Some(_) => Known::Running,
    }
}
