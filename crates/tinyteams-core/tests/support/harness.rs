//! A small in-memory host that drives one agent for each dispatched message.
//!
//! The harness deliberately owns the journal and model engines. The library
//! under test contributes only conversation identity, preserving the same
//! host/library boundary used by production consumers.

use tinyteams_core::chat::same_conversation;

/// One attributed entry in the host-owned session journal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Message {
    pub(crate) sequence: usize,
    pub(crate) chat: Option<String>,
    pub(crate) author: String,
    pub(crate) content: String,
}

/// A model-backed or deterministic chat agent driven by the harness.
pub(crate) trait AgentEngine {
    fn name(&self) -> &str;

    fn respond(&mut self, transcript: &[Message]) -> Result<String, String>;
}

/// A host-owned append-only journal plus its one-turn dispatch edge.
#[derive(Debug, Default)]
pub(crate) struct ChatHarness {
    journal: Vec<Message>,
}

impl ChatHarness {
    pub(crate) fn send(
        &mut self,
        chat: Option<&str>,
        author: impl Into<String>,
        content: impl Into<String>,
    ) -> usize {
        let sequence = self.journal.len();
        self.journal.push(Message {
            sequence,
            chat: chat.map(str::to_owned),
            author: author.into(),
            content: content.into(),
        });
        sequence
    }

    pub(crate) fn dispatch(
        &mut self,
        chat: Option<&str>,
        agent: &mut impl AgentEngine,
    ) -> Result<usize, String> {
        let transcript = self.transcript(chat);
        let author = agent.name().to_owned();
        let response = agent.respond(&transcript)?;
        Ok(self.send(chat, author, response))
    }

    pub(crate) fn transcript(&self, chat: Option<&str>) -> Vec<Message> {
        self.journal
            .iter()
            .filter(|message| same_conversation(message.chat.as_deref(), chat))
            .cloned()
            .collect()
    }

    pub(crate) fn journal(&self) -> &[Message] {
        &self.journal
    }
}
