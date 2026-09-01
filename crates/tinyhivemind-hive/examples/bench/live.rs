//! Drive one episode through a real agent CLI.
//!
//! The simulated arms measure the protocol; this measures whether a real agent
//! can hold the protocol. Any command that reads a prompt as its final
//! argument and prints an answer works, which covers the obvious ones:
//!
//! ```sh
//! cargo run -p tinyhivemind-hive --example bench -- \
//!   --agent-cmd "opencode run --pure -m openrouter/~openai/gpt-mini-latest"
//! cargo run -p tinyhivemind-hive --example bench -- --agent-cmd "claude -p"
//! cargo run -p tinyhivemind-hive --example bench -- --agent-cmd "codex exec"
//! ```
//!
//! One process per turn. The library still authorizes exactly one speaker per
//! step, so the number of processes an episode can start is bounded by the
//! turn budget and by nothing else.

use std::process::Command;

use tinyhivemind_hive::{
    HiveTurn, Phase, QuorumPolicy, Sequence, SessionAuthor, SessionMessage, Visibility,
    quorum::standings, trace::resolve,
};

use crate::run::Participant;

/// The moves available while the room is still deliberating.
///
/// `!commit` is deliberately absent. The library authorizes a commit turn by
/// setting [`Phase::Commit`], and a `!commit` trace deposited before that adds
/// no supporter to anything — a live room that reaches for it early spends its
/// whole budget recording a decision it never actually reached. Offering only
/// the moves that count in this phase is the fix, and it is one a host owes
/// its agents rather than something the library can impose.
const DELIBERATE_PROTOCOL: &str = "\
Reply with ONE line only, beginning with exactly one of these markers:
!propose #topic <one sentence>   put a new option on the floor
!support #topic ^N <why>         back an option, citing message N as grounds
!object >N ^M <why>              object to message N, citing message M
!evidence ^N <fact>              add grounds without taking a side
!question <what you need>        ask for something not yet established
The # on a topic and the ^ on a citation are part of the grammar: `!propose \
#canary ...` names an option, `!propose canary ...` names nothing and is \
discarded. A support with no ^citation does not count, and only support moves \
an option towards a decision. Do not write !commit: the room has not reached a \
decision yet, and a commit line now counts for nothing. Write nothing before \
or after the single marker line.";

/// The move available once the room has reached quorum.
const COMMIT_PROTOCOL: &str = "\
The room has reached quorum. Reply with ONE line only, recording the option \
that carried:
!commit #topic ^N <why>          record the decision, citing message N
Keep the # on the topic and the ^ on the citation; without them the line \
records nothing. Use the topic the room actually settled on, not the one you \
would have preferred. Write nothing before or after the single marker line.";

/// A participant backed by an external agent command.
pub(crate) struct LiveAgent {
    id: String,
    role: String,
    program: String,
    args: Vec<String>,
    /// The room's quorum rule. It is public — a participant is entitled to
    /// know how many grounded supporters settle a question — and the harness
    /// reads the medium through the library's own fold to report it.
    quorum: QuorumPolicy,
}

impl LiveAgent {
    /// Build a participant from a command line such as `opencode run`.
    ///
    /// The prompt is appended as the final argument.
    pub(crate) fn new(id: &str, role: &str, command: &str, quorum: QuorumPolicy) -> Option<Self> {
        let mut words = command.split_whitespace().map(str::to_owned);
        let program = words.next()?;
        Some(Self {
            id: id.to_owned(),
            role: role.to_owned(),
            program,
            args: words.collect(),
            quorum,
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
        let protocol = match turn.phase {
            Phase::Deliberate => DELIBERATE_PROTOCOL,
            Phase::Commit => COMMIT_PROTOCOL,
        };
        format!(
            "You are @{}, the {} on a small team. {sight}\n\n{protocol}\n\n{}{}\n\
             Shared attributed transcript:\n{transcript}\n\nYour one line:",
            self.id,
            self.role,
            self.floor(visible),
            self.last_line(visible),
        )
    }

    /// The topics on the floor, with the standing the library gives each.
    ///
    /// Two live failures come from a participant not knowing this. Models coin
    /// a fresh topic id for an idea the room already has one for — `#rollout`
    /// and `#rollout-strategy` in the same episode — and support split across
    /// two names for one idea never adds up to a quorum. And a participant
    /// that cannot see how far an option is from carrying has no way to know
    /// that one more supporter would settle it. Both are cheap to repair, and
    /// repairing them is the host's job: the standings are folded here with
    /// [`standings`], the same function the episode uses.
    fn floor(&self, visible: &[&SessionMessage]) -> String {
        let traces: Vec<_> = visible
            .iter()
            .flat_map(|message| resolve(&message.content, None, &message.author, message.sequence))
            .collect();
        let at = visible
            .last()
            .map_or(Sequence(0), |message| message.sequence);
        let Ok(standings) = standings(&traces, at, &self.quorum) else {
            return String::new();
        };
        if standings.is_empty() {
            return format!(
                "No option is on the floor yet. The topic id you coin becomes the room's name \
                 for that option, so keep it short. An option carries once {} different \
                 members have backed it with grounds.\n\n",
                self.quorum.threshold,
            );
        }
        let floor = standings
            .iter()
            .map(|standing| {
                format!(
                    "#{} — {} of the {} supporters it needs ({})",
                    standing.topic,
                    standing.supporters.len(),
                    self.quorum.threshold,
                    if standing.supporters.is_empty() {
                        "nobody counted yet".to_owned()
                    } else {
                        standing.supporters.join(", ")
                    },
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "Options already on the floor. Reuse one of these ids exactly if your point is about \
             it — support split across two names for one idea never adds up to a decision — and \
             only coin a new id for a genuinely different option:\n{floor}\n\n",
        )
    }

    /// The marker line this participant last authored, if any.
    ///
    /// Live models restate their own previous line verbatim when they have
    /// nothing new: one run spent four consecutive turns on the same
    /// `!question`. The protocol's `repetition_cap` damps a restated *support*
    /// and cannot see this, so the participant is shown what it already said
    /// and told not to say it again.
    fn last_line(&self, visible: &[&SessionMessage]) -> String {
        let own = visible
            .iter()
            .rev()
            .find(|message| match &message.author {
                SessionAuthor::Agent { id, .. } => id == &self.id,
                SessionAuthor::Operator
                | SessionAuthor::Person { .. }
                | SessionAuthor::System { .. } => false,
            })
            .map(|message| message.content.trim());
        own.map_or_else(String::new, |line| {
            format!("You already said this, so do not repeat it — say something that moves the room on:\n{line}\n\n")
        })
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
        let text = plain(&String::from_utf8_lossy(&output.stdout));
        // Take the marker line if the agent wrapped it in prose or a banner; a
        // turn that deposits no trace is still a legal turn, so prose falls
        // through to the first thing the agent actually said.
        let marker = text
            .lines()
            .map(str::trim)
            .find(|line| line.starts_with('!'));
        let answer = marker
            .or_else(|| {
                text.lines()
                    .map(str::trim)
                    .find(|line| !line.is_empty() && !line.starts_with('>'))
            })
            .unwrap_or("(no answer)");
        Ok(answer.to_owned())
    }
}

/// Strip ANSI escape sequences from a CLI's output.
///
/// Real agent CLIs colour what they print and draw a banner around it. A
/// marker line preceded by a colour reset is still a marker line, and the
/// grammar reads the text rather than the terminal.
fn plain(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '\u{1b}' {
            plain.push(character);
            continue;
        }
        // CSI sequences end at their final byte in `@`..=`~`; anything else
        // after the escape is a two-character sequence.
        if characters.next() == Some('[') {
            for byte in characters.by_ref() {
                if ('@'..='~').contains(&byte) {
                    break;
                }
            }
        }
    }
    plain
}
