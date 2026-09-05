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
//!
//! [`http`](crate::http) drives the same prompt state directly against an
//! HTTP endpoint instead of a CLI, so [`AgentPrompt`] — everything about a
//! seat except how its answer is fetched — is shared between the two.

use std::process::Command;
use std::sync::OnceLock;

use tinyhivemind_hive::{
    DirectoryPolicy, HiveTurn, Phase, QuorumPolicy, Sequence, SessionAuthor, SessionMessage,
    Visibility, directory::directory, quorum::standings, trace::resolve,
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
!evidence #topic ^N  then a fact, adding grounds without taking a side; keep the # when the fact bears on a named option
!defer #topic  then who should answer instead, when this is not your area
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
now counts for nothing. !defer costs you this turn and adds no support to \
anything. Use it when the question on the floor turns on something you do not \
hold and somebody here does: a confident guess from outside your area is \
worse for the room than saying so and standing aside. Write nothing before or \
after the single marker line.";

/// The move available once the room has reached quorum.
const COMMIT_PROTOCOL: &str = "\
The room has reached quorum. Reply with ONE line only, recording the option \
that carried:
!commit #topic ^N  then why, citing message N
Keep the # on the topic and the ^ on the citation; without them the line \
records nothing. Angle brackets are not part of the line — write the sentence \
itself. Use the topic the room actually settled on, not the one you would have \
preferred. Write nothing before or after the single marker line.";

/// The rules the room's earned-expertise directory is read under.
///
/// Passed through [`directory_block`] rather than baked into the deliberation
/// rules, because a room with no recorded grounds yet has nothing to say here
/// and should say nothing.
const KNOWS_RULES: &str = "\
It is earned rather than declared: a member is named against a topic here \
only after actually producing grounds for it in this room, never because it \
claims expertise for itself. It is a prior and not an answer — reaching for \
the member the room has already seen ground a topic does not excuse you from \
writing your own !support or !evidence citing what is actually on the floor.";

/// Render the room's directory of who has grounded which topic.
///
/// This is passed through [`AgentPrompt::prompt_with`] as `extra`, the same
/// seam [`LiveDeskAgent::directory`] uses for the federation's own move, so a
/// participant reads it alongside the ordinary moves rather than as an
/// afterthought. Taking `lines` as prose rather than the library's own
/// directory type keeps this module free of a dependency on it; the caller
/// wires `Directory::lines()` in.
#[must_use]
pub(crate) fn directory_block(lines: &[String]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    format!(
        "The room keeps a record of who has produced grounded facts on which topics. \
         {KNOWS_RULES}\n\nWhat the room has learned so far:\n{}\n",
        lines.join("\n"),
    )
}

/// Everything about a seat except how its answer is fetched.
///
/// [`LiveAgent`] and `HttpAgent` differ only in the last mile — one shells out
/// to a CLI, the other posts to an HTTP endpoint — and this holds the part
/// that is identical between them: the room's quorum rule, this member's
/// private facts, and the prompt assembly that reads both. Keeping it as one
/// type is what makes the parsing identical for a CLI seat and an HTTP seat.
pub(crate) struct AgentPrompt {
    id: String,
    role: String,
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

impl AgentPrompt {
    /// Build the prompt state for one seat.
    pub(crate) fn new(id: &str, role: &str, quorum: QuorumPolicy, private: String) -> Self {
        Self {
            id: id.to_owned(),
            role: role.to_owned(),
            quorum,
            private,
        }
    }

    /// Canonical agent id, matching a desk member.
    pub(crate) fn id(&self) -> &str {
        &self.id
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
            Phase::Deliberate => {
                let directory = Self::earned_directory(visible);
                format!("{DELIBERATE_MOVES}\n{extra}\n{directory}{DELIBERATE_RULES}")
            }
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

    /// The room's earned-expertise directory, rendered for a prompt.
    ///
    /// Folded fresh from `visible` on every turn with the library's own
    /// [`directory`] — the same pattern [`Self::floor`] uses for standings —
    /// rather than threaded through as a parameter, so a participant's speak
    /// implementation needs no change to pick this up: [`Trace`]s already
    /// carry everything the fold reads. Empty for a room where nobody has
    /// deposited anything gradeable yet, in which case [`directory_block`]
    /// renders nothing.
    ///
    /// [`Trace`]: tinyhivemind_hive::trace::Trace
    fn earned_directory(visible: &[&SessionMessage]) -> String {
        let traces: Vec<_> = visible
            .iter()
            .flat_map(|message| resolve(&message.content, None, &message.author, message.sequence))
            .collect();
        let at = visible
            .last()
            .map_or(Sequence(0), |message| message.sequence);
        let Ok(folded) = directory(&traces, at, &DirectoryPolicy::DEFAULT, &[]) else {
            return String::new();
        };
        directory_block(&folded.lines())
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

/// The external timeout wrapper found on `PATH`, if any.
///
/// macOS ships neither `timeout` nor `gtimeout` by default — the latter comes
/// from Homebrew's `coreutils` — so a run without either falls back to today's
/// behaviour (no per-turn deadline) rather than failing outright. The warning
/// is printed once no matter how many seats are built.
fn timeout_binary() -> Option<&'static str> {
    static BIN: OnceLock<Option<&'static str>> = OnceLock::new();
    *BIN.get_or_init(|| {
        for candidate in ["gtimeout", "timeout"] {
            if on_path(candidate) {
                return Some(candidate);
            }
        }
        eprintln!(
            "warning: neither `gtimeout` nor `timeout` is on PATH; live agent turns run with no \
             per-turn timeout (install coreutils for `gtimeout` on macOS)",
        );
        None
    })
}

/// Whether `program` resolves on `PATH`.
fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

/// A participant backed by an external agent command.
pub(crate) struct LiveAgent {
    prompt: AgentPrompt,
    program: String,
    args: Vec<String>,
    /// Per-turn timeout in seconds, applied through `timeout`/`gtimeout` when
    /// one is on `PATH`.
    timeout_secs: u64,
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
        timeout_secs: u64,
    ) -> Option<Self> {
        let (program, args) = split_command(command)?;
        Some(Self {
            prompt: AgentPrompt::new(id, role, quorum, private),
            program,
            args,
            timeout_secs,
        })
    }

    /// What only this member knows, as a block for a prompt.
    pub(crate) fn private(&self) -> &str {
        self.prompt.private()
    }

    /// Render exactly what this turn is allowed to see.
    pub(crate) fn prompt(&self, turn: &HiveTurn, visible: &[&SessionMessage]) -> String {
        self.prompt.prompt(turn, visible)
    }

    /// The same, with one extra block of moves offered alongside the protocol.
    pub(crate) fn prompt_with(
        &self,
        turn: &HiveTurn,
        visible: &[&SessionMessage],
        extra: &str,
    ) -> String {
        self.prompt.prompt_with(turn, visible, extra)
    }

    /// Run one prompt through the agent process and take its one line.
    ///
    /// One retry on a non-zero exit or an empty answer: a live process
    /// occasionally drops a turn for reasons that have nothing to do with the
    /// protocol — a cold start, a rate limit, a flaky network call — and
    /// spending the episode's whole turn on that is a worse failure than
    /// spending one extra process on a retry.
    ///
    /// # Errors
    ///
    /// Returns the second attempt's failure: the process could not be
    /// started, or exited non-zero.
    pub(crate) fn line(&self, prompt: &str) -> Result<String, String> {
        match self.attempt(prompt) {
            Ok(answer) if answer != "(no answer)" => Ok(answer),
            _ => self.attempt(prompt),
        }
    }

    fn attempt(&self, prompt: &str) -> Result<String, String> {
        let mut command = timeout_binary().map_or_else(
            || Command::new(&self.program),
            |bin| {
                let mut wrapped = Command::new(bin);
                wrapped
                    .arg(self.timeout_secs.to_string())
                    .arg(&self.program);
                wrapped
            },
        );
        let output = command
            .args(&self.args)
            .arg(prompt)
            .output()
            .map_err(|error| format!("could not run {}: {error}", self.program))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let head = stderr.lines().take(3).collect::<Vec<_>>().join(" | ");
            return Err(format!(
                "{} exited with {}: {head}",
                self.program, output.status,
            ));
        }
        Ok(marker_line(&String::from_utf8_lossy(&output.stdout)))
    }
}

/// The one line a seat's answer contributes to the transcript.
///
/// Colour and banners are stripped first, then the marker line is taken if
/// the agent wrapped it in prose. A turn that deposits no trace is still a
/// legal turn, so prose falls through to the first thing the agent actually
/// said.
///
/// Shared with [`crate::http`] rather than reimplemented there: an HTTP seat
/// and a CLI seat have to parse identically or a room that mixes the two is
/// not one room. The escape stripping is inert on a JSON body that never
/// carried an escape, so applying it to both costs nothing.
pub(crate) fn marker_line(text: &str) -> String {
    let text = plain(text);
    let marker = text
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with('!') || line.starts_with('@'));
    marker
        .or_else(|| {
            text.lines()
                .map(str::trim)
                .find(|line| !line.is_empty() && !line.starts_with('>'))
        })
        .unwrap_or("(no answer)")
        .to_owned()
}

impl Participant for LiveAgent {
    fn id(&self) -> &str {
        self.prompt.id()
    }

    fn speak(&mut self, turn: &HiveTurn, visible: &[&SessionMessage]) -> Result<String, String> {
        self.line(&self.prompt(turn, visible))
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

/// Where a live seat's answer comes from, for the independent poll.
///
/// The poll is the matched-budget control, and a control run through a
/// different backend than the seats it is scored against would not be
/// matched: an HTTP seat and a CLI seat cost different things and answer
/// under different failure modes, so [`poll`] takes whichever backend the
/// seats themselves ran under.
pub(crate) enum Backend {
    /// One process per member, such as `claude -p` or `codex exec`.
    Cli(String),
    /// Direct HTTP against an `OpenAI`- or `Anthropic`-shaped endpoint, one
    /// model for every member.
    Http {
        /// The endpoint and credentials.
        config: crate::http::HttpConfig,
        /// The model every polled member answers under.
        model: String,
    },
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
pub(crate) fn poll(
    scenario: &Scenario,
    backend: &Backend,
) -> Result<Vec<(String, String)>, String> {
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
        let text = match backend {
            Backend::Cli(command) => {
                let (program, args) = split_command(command).ok_or("empty agent command")?;
                let output = Command::new(&program)
                    .args(&args)
                    .arg(prompt)
                    .output()
                    .map_err(|error| format!("could not run {program}: {error}"))?;
                if !output.status.success() {
                    return Err(format!("{program} exited with {}", output.status));
                }
                plain(&String::from_utf8_lossy(&output.stdout))
            }
            Backend::Http { config, model } => {
                // The poll has never reported its own token spend, so the
                // handle is a sink; the CLI arm beside it accounts for
                // nothing either, and one of the two accounting would make
                // the matched-budget comparison uneven rather than better.
                crate::http::ask(config, model, &prompt, &crate::http::UsageHandle::default())?
            }
        };
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
pub(crate) const CROSS_PROTOCOL: &str = "\
@#deskid  then your question, asking another desk — this is a marker like the \
others and a legal line on its own";

/// What a member needs to know about that move to use it well.
pub(crate) const CROSS_RULES: &str = "\
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
    fn one_line(&self, prompt: &str) -> Result<String, String> {
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
        self.one_line(&prompt)
    }

    fn answer(
        &mut self,
        incoming: &Referral,
        visible: &[&SessionMessage],
    ) -> Result<String, String> {
        let transcript = AgentPrompt::render(visible);
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
        self.one_line(&prompt)
    }
}
