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
use crate::scenario::Scenario;
use crate::swarm::SwarmMember;
use tinyhivemind_hive::referral::{Referral, ReferralKind};

/// The moves available while the room is still deliberating.
///
/// `!commit` is deliberately absent. The library authorizes a commit turn by
/// setting [`Phase::Commit`], and a `!commit` trace deposited before that adds
/// no supporter to anything — a live room that reaches for it early spends its
/// whole budget recording a decision it never actually reached. Offering only
/// the moves that count in this phase is the fix, and it is one a host owes
/// its agents rather than something the library can impose.
const DELIBERATE_MOVES: &str = "\
Reply with ONE line only, beginning with exactly one of these markers:
!propose #topic  then one sentence putting a new option on the floor
!support #topic ^N  then why, citing message N as grounds
!object >N ^M  then why, objecting to message N and citing message M
!evidence ^N  then a fact, adding grounds without taking a side
!question  then what you need that nobody has established";

/// The rules those moves are read under.
const DELIBERATE_RULES: &str = "\
The # on a topic and the ^ on a citation are part of the grammar: `!propose \
#canary ...` names an option, `!propose canary ...` names nothing and is \
discarded. A support with no ^citation does not count, and only support moves \
an option towards a decision. Angle brackets are not part of any line — write \
the sentence itself, not a placeholder in brackets. The marker is what the \
room counts and your prose is not, so never write !support for one option \
while arguing for another: put the marker on the option you actually mean. Do \
not write !commit: the room has not reached a decision yet, and a commit line \
now counts for nothing. Write nothing before or after the single marker line.";

/// The move available once the room has reached quorum.
const COMMIT_PROTOCOL: &str = "\
The room has reached quorum. Reply with ONE line only, recording the option \
that carried:
!commit #topic ^N  then why, citing message N
Keep the # on the topic and the ^ on the citation; without them the line \
records nothing. Angle brackets are not part of the line — write the sentence \
itself. Use the topic the room actually settled on, not the one you would have \
preferred. Write nothing before or after the single marker line.";

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
    /// What only this member knows, or empty for a room with no scenario.
    ///
    /// It is deliberately not appended to the shared journal. A fact every
    /// member can already read is not private information, and a room whose
    /// members all start from the same facts has nothing to pool — which is
    /// the failure mode that makes most multi-agent results uninteresting.
    private: String,
}

impl LiveAgent {
    /// Build a participant from a command line such as `opencode run`.
    ///
    /// The prompt is appended as the final argument.
    pub(crate) fn new(
        id: &str,
        role: &str,
        command: &str,
        quorum: QuorumPolicy,
        private: String,
    ) -> Option<Self> {
        let (program, args) = split_command(command)?;
        Some(Self {
            id: id.to_owned(),
            role: role.to_owned(),
            program,
            args,
            quorum,
            private,
        })
    }

    /// What only this member knows, as a block for a prompt.
    ///
    /// A referred turn needs this as much as an ordinary one does, and the
    /// first live federation forgot it: the answering agent was handed the
    /// question and its desk's transcript and nothing else, so it could only
    /// argue from what the desk had already said. It argued. The one desk that
    /// asked a question got a rebuttal instead of the fact it asked for, and
    /// adopted the answering desk's hypothesis.
    pub(crate) fn private(&self) -> &str {
        &self.private
    }

    /// Render an attributed transcript the way every prompt here shows one.
    pub(crate) fn render(visible: &[&SessionMessage]) -> String {
        visible
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
            .join("\n")
    }

    /// Run one prompt through the agent process and take its one line.
    ///
    /// # Errors
    ///
    /// Returns the process failure, or a non-zero exit.
    pub(crate) fn line(&self, prompt: String) -> Result<String, String> {
        let output = Command::new(&self.program)
            .args(&self.args)
            .arg(prompt)
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
            .find(|line| line.starts_with('!') || line.starts_with('@'));
        let answer = marker
            .or_else(|| {
                text.lines()
                    .map(str::trim)
                    .find(|line| !line.is_empty() && !line.starts_with('>'))
            })
            .unwrap_or("(no answer)");
        Ok(answer.to_owned())
    }

    /// Render exactly what this turn is allowed to see.
    pub(crate) fn prompt(&self, turn: &HiveTurn, visible: &[&SessionMessage]) -> String {
        self.prompt_with(turn, visible, "")
    }

    /// The same, with one extra block of moves offered alongside the protocol.
    ///
    /// `extra` lands *inside* the protocol block rather than before the whole
    /// prompt. That placement is not cosmetic. The first federated live run put
    /// the cross-channel move above everything else, so the last instruction
    /// the model read was still "reply with exactly one of these markers" —
    /// and across twenty-seven turns on three desks, not one agent addressed
    /// another channel. A move offered before the list of moves is not offered.
    pub(crate) fn prompt_with(
        &self,
        turn: &HiveTurn,
        visible: &[&SessionMessage],
        extra: &str,
    ) -> String {
        let transcript = Self::render(visible);
        let sight = match turn.visibility {
            Visibility::Blind => "You cannot yet see your peers' positions. Form your own first.",
            Visibility::Full => "You can see the whole room.",
        };
        // `extra` lands between the list of moves and the rules they are read
        // under, so an extra move reads as one of the markers rather than as an
        // afterthought. The commit phase offers nothing extra: the room has
        // already reached a decision and there is nothing left to ask anybody.
        let protocol = match turn.phase {
            Phase::Deliberate => format!("{DELIBERATE_MOVES}\n{extra}\n{DELIBERATE_RULES}"),
            Phase::Commit => COMMIT_PROTOCOL.to_owned(),
        };
        format!(
            "You are @{}, the {} on a small team. {sight}\n\n{}\n{protocol}\n\n{}{}\n\
             Shared attributed transcript:\n{transcript}\n\nYour one line:",
            self.id,
            self.role,
            self.private,
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
            let naming = if self.private.is_empty() {
                "The topic id you coin becomes the room's name for that option, so keep it short."
            } else {
                "Use one of the ids the brief already names; a fresh id for an option the brief \
                 has a name for splits the room's support across two names for one idea."
            };
            return format!(
                "No option is on the floor yet. {naming} An option carries once {} different \
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
        self.line(self.prompt(turn, visible))
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

/// Split an agent command line into its program and leading arguments.
fn split_command(command: &str) -> Option<(String, Vec<String>)> {
    let mut words = command.split_whitespace().map(str::to_owned);
    let program = words.next()?;
    Some((program, words.collect()))
}

/// The matched-budget control, run against the same real agents.
///
/// Every member answers the same brief alone, seeing its own private facts and
/// nobody else's line, and the room's answer is the plurality. This is the arm
/// deliberation has to beat, and on a hidden profile it is the arm that cannot
/// win: each member's own facts point at the decoy, so a vote returns the
/// decoy however many voters it polls.
///
/// # Errors
///
/// Returns a participant's own failure, such as an agent process that did not
/// answer.
pub(crate) fn poll(scenario: &Scenario, command: &str) -> Result<Vec<(String, String)>, String> {
    let (program, args) = split_command(command).ok_or("empty agent command")?;
    let mut picks = Vec::new();
    for agent in &scenario.agents {
        let prompt = format!(
            "You are @{}, the {} on a small team. You are answering alone: nobody else's view \
             is available to you and yours will not be shown to them.\n\n{}\n{}\n\
             Reply with ONE line and nothing else: the id of the option you would ship, \
             written as #id.",
            agent.id,
            agent.role,
            Scenario::private_brief(agent),
            scenario.brief(),
        );
        let output = Command::new(&program)
            .args(&args)
            .arg(prompt)
            .output()
            .map_err(|error| format!("could not run {program}: {error}"))?;
        if !output.status.success() {
            return Err(format!("{program} exited with {}", output.status));
        }
        let text = plain(&String::from_utf8_lossy(&output.stdout));
        let pick = scenario
            .options
            .iter()
            .find(|option| text.contains(&format!("#{}", option.id)))
            .map_or_else(|| "(none)".to_owned(), |option| option.id.clone());
        picks.push((agent.id.clone(), pick));
    }
    Ok(picks)
}

/// The one extra move a member of a *federation* has.
///
/// It is written as an ordinary mention because that is what it is: the host
/// reads the line with the same mention grammar it reads every other line
/// with, and `referral` decides where the turn lands. Nothing about this move
/// is special-cased, which is the point — an agent that writes `@#platform`
/// into a sentence has asked another channel a question whether or not it
/// meant to invoke a protocol.
const CROSS_PROTOCOL: &str = "\
@#deskid  then your question, asking another desk — this is a marker like the \
others and a legal line on its own";

/// What a member needs to know about that move to use it well.
const CROSS_RULES: &str = "\
You are not alone. The other desks below are working the same problem in their \
own channels and you cannot see their transcripts, so a fact one of them holds \
is a fact this desk does not have and cannot deduce. `@#deskid` runs one turn \
on that desk and their answer comes back here as one line. It costs you this \
turn, so ask only for what you cannot settle here — but do ask. Deciding on \
this desk's evidence alone, when the evidence that would change your mind is \
one question away on a desk you can address, is the exact failure this \
arrangement exists to prevent. Put your own reading in the question; a desk \
that has to guess what you already know answers a worse question.";

/// A member of a federation backed by an external agent command.
///
/// It is a [`LiveAgent`] that also knows which channel it is on and who the
/// other channels are. Everything else — the prompt, the parsing, the one
/// process per turn — is unchanged.
pub(crate) struct LiveDeskAgent {
    agent: LiveAgent,
    /// This member's own desk, by display name.
    here: String,
    /// Every other channel, id and display name.
    peers: Vec<(String, String)>,
}

impl LiveDeskAgent {
    /// Seat one live agent on a channel.
    pub(crate) fn new(agent: LiveAgent, here: String, peers: Vec<(String, String)>) -> Self {
        Self { agent, here, peers }
    }

    /// Run one prompt through the agent process and take its one line.
    fn one_line(&self, prompt: String) -> Result<String, String> {
        self.agent.line(prompt)
    }

    /// The sentence naming the channels this member may reach.
    fn directory(&self) -> String {
        let named = self
            .peers
            .iter()
            .map(|(id, name)| format!("@#{id} — the {name} desk"))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "{CROSS_PROTOCOL}\n\n{CROSS_RULES}\nYou are on the {} desk. The desks you can \
             address, and how:\n{named}\n",
            self.here,
        )
    }
}

impl SwarmMember for LiveDeskAgent {
    fn id(&self) -> &str {
        self.agent.id()
    }

    fn speak(
        &mut self,
        turn: &tinyhivemind_hive::HiveTurn,
        visible: &[&SessionMessage],
    ) -> Result<String, String> {
        // The federation's own move is offered alongside the ordinary ones,
        // and the agent decides. The harness never writes a mention on an
        // agent's behalf in this arm; if no cross-channel message happens,
        // that is a finding about the agents rather than about the protocol.
        let prompt = self.agent.prompt_with(turn, visible, &self.directory());
        self.one_line(prompt)
    }

    fn answer(
        &mut self,
        incoming: &Referral,
        visible: &[&SessionMessage],
    ) -> Result<String, String> {
        let transcript = LiveAgent::render(visible);
        let asked = match incoming.kind {
            ReferralKind::Forward => format!(
                "@{} on another desk has asked your desk this, and your answer will be posted \
                 here and carried back to them:\n{}\n\nAnswer it in ONE line, beginning with \
                 !evidence. They cannot see anything on this desk, so state the facts they need \
                 — including the ones above that only you hold — rather than your conclusion \
                 from them. A desk that asks for a number and receives an argument has learned \
                 nothing it can check. Do not ask a question back and do not tell them which \
                 option to pick.",
                incoming.source_id, incoming.content,
            ),
            ReferralKind::Return => format!(
                "You asked another desk a question and this is what came back:\n{}\n\nRelay it \
                 to your own desk in ONE line, beginning with !evidence, stating what they told \
                 you. Do not add anything they did not say.",
                incoming.content,
            ),
        };
        let prompt = format!(
            "You are @{}, on the {} desk.\n\n{}\n{asked}\n\nYour desk's transcript so far:\n\
             {transcript}\n\nYour one line:",
            self.agent.id(),
            self.here,
            self.agent.private(),
        );
        self.one_line(prompt)
    }
}
