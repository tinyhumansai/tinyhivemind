//! The in-memory started and finished records of calls that act outside the
//! coordinator's transaction: executable tools, `create_agent`, and every
//! call a hook matches.
//!
//! Each seat keeps the records of one turn: its running one. A call is
//! recorded only after the coordinator confirms, inside the ledger's lock,
//! that the call's turn is the seat's running turn, so a call that was held
//! (by an approver, a hook) while its turn ended can neither run nor replace
//! the newer turn's records. Records of a turn that is no longer running are
//! dropped by [`Ledger::purge`], and a withdrawn seat's by [`Ledger::forget`].
//! A seat's records are bounded in count and bytes, and a call past either
//! bound is refused, never recorded by evicting another. The records are
//! this process's alone: at most once holds within one server process.
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

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
    /// Its turn is not the seat's running turn.
    Stale,
    /// The seat's records are at their bound.
    Full,
}

#[derive(Debug)]
struct Entry {
    tool: String,
    arguments: Value,
    outcome: Option<Outcome>,
    bytes: usize,
}

#[derive(Debug, Default)]
struct Turn {
    turn: String,
    calls: HashMap<String, Entry>,
    bytes: usize,
}

/// Every seat's records for its running turn.
#[derive(Debug)]
pub(crate) struct Ledger {
    seats: Mutex<HashMap<String, Turn>>,
    max_records: usize,
    max_bytes: usize,
}

impl Ledger {
    /// A ledger holding at most `max_records` records and `max_bytes` bytes
    /// of arguments and results per seat.
    pub(crate) fn new(max_records: usize, max_bytes: usize) -> Self {
        Self {
            seats: Mutex::default(),
            max_records,
            max_bytes,
        }
    }

    /// What is known of `identity` as a call of `tool` with `arguments`.
    /// Looking changes nothing.
    pub(crate) fn lookup(&self, identity: &Identity, tool: &str, arguments: &Value) -> Known {
        let seats = self.seats.lock().unwrap_or_else(PoisonError::into_inner);
        seats
            .get(&identity.seat)
            .filter(|turn| turn.turn == identity.turn)
            .map_or(Known::New, |turn| {
                known(&turn.calls, &identity.call, tool, arguments)
            })
    }

    /// Record `identity` as started, if `running` (asked under the lock) says
    /// its turn is the seat's running turn and it is not known already.
    pub(crate) fn start(
        &self,
        identity: &Identity,
        tool: &str,
        arguments: &Value,
        running: impl FnOnce() -> Option<String>,
    ) -> Known {
        let mut seats = self.seats.lock().unwrap_or_else(PoisonError::into_inner);
        if running().as_deref() != Some(identity.turn.as_str()) {
            return Known::Stale;
        }
        let turn = seats.entry(identity.seat.clone()).or_default();
        if turn.turn != identity.turn {
            // The stored turn is not running, so it has ended.
            *turn = Turn {
                turn: identity.turn.clone(),
                ..Turn::default()
            };
        }
        let known = known(&turn.calls, &identity.call, tool, arguments);
        if known != Known::New {
            return known;
        }
        let bytes = identity.call.len() + tool.len() + arguments.to_string().len();
        if turn.calls.len() >= self.max_records || turn.bytes + bytes > self.max_bytes {
            return Known::Full;
        }
        turn.bytes += bytes;
        turn.calls.insert(
            identity.call.clone(),
            Entry {
                tool: tool.into(),
                arguments: arguments.clone(),
                outcome: None,
                bytes,
            },
        );
        Known::New
    }

    /// Record what a started call returned. A call whose turn has since been
    /// replaced has no record left to finish.
    pub(crate) fn finish(&self, identity: &Identity, outcome: Outcome) {
        self.with_entry(identity, |turn, call| {
            if let Some(entry) = turn.calls.get_mut(call) {
                entry.bytes += outcome.text.len();
                turn.bytes += outcome.text.len();
                entry.outcome = Some(outcome);
            }
        });
    }

    /// Drop the record of a started call that was refused before any hook or
    /// tool ran, so that it records nothing.
    pub(crate) fn abandon(&self, identity: &Identity) {
        self.with_entry(identity, |turn, call| {
            if let Some(entry) = turn.calls.remove(call) {
                turn.bytes -= entry.bytes;
            }
        });
    }

    /// Drop every record of `seat`.
    pub(crate) fn forget(&self, seat: &str) {
        let mut seats = self.seats.lock().unwrap_or_else(PoisonError::into_inner);
        seats.remove(seat);
    }

    /// Drop the records of every seat whose turn `running` says has ended.
    pub(crate) fn purge(&self, running: impl Fn(&str) -> Option<String>) {
        let mut seats = self.seats.lock().unwrap_or_else(PoisonError::into_inner);
        seats.retain(|seat, turn| running(seat).as_deref() == Some(turn.turn.as_str()));
    }

    fn with_entry(&self, identity: &Identity, act: impl FnOnce(&mut Turn, &str)) {
        let mut seats = self.seats.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(turn) = seats
            .get_mut(&identity.seat)
            .filter(|turn| turn.turn == identity.turn)
        {
            act(turn, &identity.call);
        }
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
