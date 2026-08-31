//! Deterministic model engine for coordination tests.

use std::collections::VecDeque;

use super::harness::{AgentEngine, Message};

/// A deterministic mock engine that records every transcript it receives.
#[derive(Debug)]
pub(crate) struct ScriptedAgent {
    name: String,
    responses: VecDeque<Result<String, String>>,
    calls: Vec<Vec<Message>>,
}

impl ScriptedAgent {
    pub(crate) fn new(
        name: impl Into<String>,
        responses: impl IntoIterator<Item = Result<&'static str, &'static str>>,
    ) -> Self {
        Self {
            name: name.into(),
            responses: responses
                .into_iter()
                .map(|response| response.map(str::to_owned).map_err(str::to_owned))
                .collect(),
            calls: Vec::new(),
        }
    }

    pub(crate) fn calls(&self) -> &[Vec<Message>] {
        &self.calls
    }
}

impl AgentEngine for ScriptedAgent {
    fn name(&self) -> &str {
        &self.name
    }

    fn respond(&mut self, transcript: &[Message]) -> Result<String, String> {
        self.calls.push(transcript.to_vec());
        self.responses
            .pop_front()
            .unwrap_or_else(|| Err(format!("{} has no scripted response", self.name)))
    }
}
