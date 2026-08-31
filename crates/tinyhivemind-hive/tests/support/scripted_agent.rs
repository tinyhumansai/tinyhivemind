//! A deterministic participant that replays a fixed script.

#![allow(dead_code)]

use tinyhivemind::{Sequence, SessionMessage};
use tinyhivemind_hive::{
    HiveTurn, Phase,
    quorum::{ConsensusState, QuorumPolicy, consensus, standings},
    trace::read,
};

use super::hive_harness::HiveAgent;

/// A participant that speaks from a script, recording what each turn saw.
pub(crate) struct ScriptedAgent {
    id: String,
    lines: std::collections::VecDeque<Result<String, String>>,
    /// One entry per turn taken: the sequences this turn was actually shown.
    calls: Vec<Vec<u64>>,
    /// What it says once its script runs out.
    filler: String,
}

impl ScriptedAgent {
    pub(crate) fn new<'a>(id: &str, lines: impl IntoIterator<Item = &'a str>) -> Self {
        Self {
            id: id.to_owned(),
            lines: lines.into_iter().map(|line| Ok(line.to_owned())).collect(),
            calls: Vec::new(),
            filler: "!question Nothing further from me.".to_owned(),
        }
    }

    /// A participant whose first model call fails.
    pub(crate) fn failing(id: &str, message: &str) -> Self {
        let mut agent = Self::new(id, []);
        agent.lines.push_back(Err(message.to_owned()));
        agent
    }

    pub(crate) fn calls(&self) -> &[Vec<u64>] {
        &self.calls
    }
}

impl HiveAgent for ScriptedAgent {
    fn id(&self) -> &str {
        &self.id
    }

    fn speak(&mut self, turn: &HiveTurn, visible: &[&SessionMessage]) -> Result<String, String> {
        self.calls
            .push(visible.iter().map(|message| message.sequence.0).collect());
        if let Some(line) = self.lines.pop_front() {
            return line;
        }
        if turn.phase == Phase::Commit {
            return Ok(commit_line(visible));
        }
        Ok(self.filler.clone())
    }
}

/// What an out-of-script participant says once the room has moved to
/// [`Phase::Commit`]: the carried topic, recomputed from what this turn can
/// see, rather than a `!question` that would never record a decision.
fn commit_line(visible: &[&SessionMessage]) -> String {
    let messages: Vec<SessionMessage> = visible.iter().map(|message| (*message).clone()).collect();
    let traces = read(&messages);
    let at = messages
        .last()
        .map_or(Sequence(0), |message| message.sequence);
    let policy = QuorumPolicy::DEFAULT;
    if let Ok(standing) = standings(&traces, at, &policy)
        && let ConsensusState::Quorum { topic } = consensus(&standing, &policy)
    {
        return format!("!commit #{topic} Recording the decision.");
    }
    "!question Nothing further from me.".to_owned()
}
