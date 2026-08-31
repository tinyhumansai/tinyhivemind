//! Drive one episode through a real agent CLI.
//!
//! The simulated arms measure the protocol; this measures whether a real agent
//! can hold the protocol. Any command that reads a prompt as its final
//! argument and prints an answer works, which covers the obvious ones:
//!
//! ```sh
//! cargo run -p tinyhivemind-hive --example bench -- --agent-cmd "opencode run"
//! cargo run -p tinyhivemind-hive --example bench -- --agent-cmd "claude -p"
//! cargo run -p tinyhivemind-hive --example bench -- --agent-cmd "codex exec"
//! ```
//!
//! One process per turn. The library still authorizes exactly one speaker per
//! step, so the number of processes an episode can start is bounded by the
//! turn budget and by nothing else.

use std::process::Command;

use tinyhivemind_hive::{HiveTurn, Phase, SessionAuthor, SessionMessage, Visibility};

use crate::run::Participant;

/// The grammar a live participant is asked to speak.
const PROTOCOL: &str = "\
You are deliberating with peers in a shared room. Reply with ONE line only, \
beginning with exactly one of these markers:
!propose #topic <one sentence>   put a new option on the floor
!support #topic ^N <why>         back an option, citing message N as grounds
!object >N ^M <why>              object to message N, citing message M
!evidence ^N <fact>              add grounds without taking a side
!question <what you need>        ask for something not yet established
!commit #topic ^N <why>          record the decision the room has reached
Topics are short kebab-case words. A support with no ^citation does not count. \
Write nothing before or after the single marker line.";

/// A participant backed by an external agent command.
pub(crate) struct LiveAgent {
    id: String,
    role: String,
    program: String,
    args: Vec<String>,
}

impl LiveAgent {
    /// Build a participant from a command line such as `opencode run`.
    ///
    /// The prompt is appended as the final argument.
    pub(crate) fn new(id: &str, role: &str, command: &str) -> Option<Self> {
        let mut words = command.split_whitespace().map(str::to_owned);
        let program = words.next()?;
        Some(Self {
            id: id.to_owned(),
            role: role.to_owned(),
            program,
            args: words.collect(),
        })
    }

    /// Render exactly what this turn is allowed to see.
    fn prompt(&self, turn: &HiveTurn, visible: &[&SessionMessage]) -> String {
        let transcript = visible
            .iter()
            .map(|message| {
                let author = match &message.author {
                    SessionAuthor::Agent { label, .. }
                    | SessionAuthor::Person { label, .. }
                    | SessionAuthor::System { label, .. } => label.as_str(),
                    SessionAuthor::Operator => "operator",
                };
                format!("[{}] {author}: {}", message.sequence, message.content)
            })
            .collect::<Vec<_>>()
            .join("\n");
        let sight = match turn.visibility {
            Visibility::Blind => "You cannot yet see your peers' positions. Form your own first.",
            Visibility::Full => "You can see the whole room.",
        };
        let phase = match turn.phase {
            Phase::Deliberate => "The room is still deliberating.",
            Phase::Commit => "The room has reached quorum; record the decision with !commit.",
        };
        format!(
            "You are @{}, the {} on a small team. {sight} {phase}\n\n{PROTOCOL}\n\n\
             Shared attributed transcript:\n{transcript}\n\nYour one line:",
            self.id, self.role,
        )
    }
}

impl Participant for LiveAgent {
    fn id(&self) -> &str {
        &self.id
    }

    fn speak(&mut self, turn: &HiveTurn, visible: &[&SessionMessage]) -> Result<String, String> {
        let output = Command::new(&self.program)
            .args(&self.args)
            .arg(self.prompt(turn, visible))
            .output()
            .map_err(|error| format!("could not run {}: {error}", self.program))?;
        if !output.status.success() {
            return Err(format!("{} exited with {}", self.program, output.status));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        // Take the marker line if the agent wrapped it in prose; a turn that
        // deposits no trace is still a legal turn, so prose falls through.
        let marker = text
            .lines()
            .map(str::trim)
            .find(|line| line.starts_with('!'));
        let answer = marker
            .or_else(|| text.lines().map(str::trim).find(|line| !line.is_empty()))
            .unwrap_or("(no answer)");
        Ok(answer.to_owned())
    }
}
