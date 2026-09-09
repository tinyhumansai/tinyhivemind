//! Folding the room's older messages into one account it can carry.
//!
//! The library decides *when* a channel needs folding and *what* the fold may
//! cover; this is the host's side of that port — one tool-less completion,
//! through the same router the wrap-up uses, for the same reason: a fold must
//! not be able to run a command, write a file, or take a turn.
//!
//! The account it produces is never a transcript row. It is host state the
//! next prompt is composed from, and every message it stands for is still in
//! the log at its original sequence.

use crate::chat;
use std::fmt::Write as _;
use tinyhivemind::{DigestFuture, DigestRequest, Digester, SessionAuthor};

/// A digester backed by one plain chat completion.
pub(crate) struct RoomDigester {
    chat: chat::Chat,
}

impl RoomDigester {
    /// Build one against an already-configured router client.
    pub(crate) const fn new(chat: chat::Chat) -> Self {
        Self { chat }
    }
}

impl Digester for RoomDigester {
    fn digest<'a>(&'a self, request: &'a DigestRequest) -> DigestFuture<'a> {
        let prompt = compose(request);
        // A fold that fails costs the room its compaction and nothing else,
        // so every outcome that is not an answer becomes the empty string and
        // the library reads that as `DigestRejection::Empty`.
        Box::pin(async move { Ok(self.chat.complete(&prompt).await.text().to_string()) })
    }
}

/// What the folder is asked.
///
/// It is told it is *rewriting* an account rather than summarizing a
/// conversation, and told what the account is for: a seat that was not there.
/// A summary written for a reader who already knows the room says "they
/// continued the discussion"; one written for a reader who does not has to
/// carry the facts.
fn compose(request: &DigestRequest) -> String {
    let mut prompt = String::from(
        "You keep the standing account of one working desk: a single bounded text that \
         stands for everything the room said before its recent messages. It is read at the \
         top of every turn by seats that were not there, so it must carry facts rather than \
         describe activity.\n\n",
    );
    if let Some(prior) = &request.prior {
        prompt.push_str(
            "## The account so far\n\nRewrite this. Do not append to it — drop what has been \
             superseded, keep what still stands.\n\n",
        );
        prompt.push_str(prior);
        prompt.push_str("\n\n");
    }
    prompt.push_str("## What the room has said since\n");
    for message in &request.messages {
        let who = match &message.author {
            SessionAuthor::Agent { id, .. } => format!("@{id}"),
            SessionAuthor::Person { label, .. } => label.clone(),
            SessionAuthor::Operator => "operator".to_string(),
            SessionAuthor::System { kind, .. } => format!("system/{kind}"),
        };
        let _ = write!(
            prompt,
            "\n[{}] {who}: {}\n",
            message.sequence.0, message.content
        );
    }
    let _ = write!(
        prompt,
        "\n\n## Write the new account\n\
         At most {} characters. No preamble, no heading, no note about what you did — the \
         account itself and nothing else.\n\n\
         Keep, in this order:\n\
         - what has been established and verified, with the numbers and the file that holds \
           each one;\n\
         - what has been tried and ruled out, so nobody repeats it;\n\
         - what is open, and who was last working on it;\n\
         - any decision the room made about how it is working.\n\n\
         Drop: greetings, restated instructions, the room's own coordination, and anything \
         a later message superseded. Attribute a claim to the seat that made it (@id) and \
         cite the message number as ^N where it matters. Never write a number the messages \
         above do not contain.",
        request.budget_chars
    );
    prompt
}

#[cfg(test)]
mod test;
