//! A real problem for a live room to solve, and the private facts behind it.
//!
//! The simulated rooms in [`crate::sim`] carry their information as numbers: a
//! private, noisy evaluation of every option. That is the right shape for
//! measuring a protocol and the wrong shape for watching real agents, which
//! cannot hold a number they were never given a reason for. A scenario is the
//! same experiment written in prose — one shared brief, one private brief per
//! member, and a recorded ground truth — so the live room is deciding
//! something that has an answer.
//!
//! The shape that makes it worth running is a *hidden profile*: every member's
//! own facts point at the same wrong option, and the right one is reachable
//! only by pooling facts across members. A matched-budget vote cannot solve a
//! hidden profile by construction, because every independent voter answers the
//! decoy. Deliberation is the only arm that can, which is the sharpest test of
//! whether the protocol earns its turns.
//!
//! The file format is a handful of lines:
//!
//! ```text
//! task: what the room must decide
//! truth: the option id that is genuinely right
//!
//! [option rollback]
//! One sentence describing it.
//!
//! [agent planner]
//! role: release manager, who owns what can and cannot be shipped
//! knows: a fact this member holds and nobody else does
//! ```

use std::fmt::Write as _;

/// One option the room may settle on.
pub(crate) struct ScenarioOption {
    /// The topic id the room uses for it, without the `#`.
    pub(crate) id: String,
    /// One sentence saying what it is.
    pub(crate) description: String,
}

/// One member of the room, and what only it knows.
pub(crate) struct ScenarioAgent {
    /// Canonical agent id, matching a desk member.
    pub(crate) id: String,
    /// What this member is on the team to do.
    pub(crate) role: String,
    /// Facts held by this member and by nobody else.
    pub(crate) knows: Vec<String>,
}

/// A problem a live room is asked to decide.
pub(crate) struct Scenario {
    /// The brief every member sees, appended to the shared journal.
    pub(crate) task: String,
    /// The option that is genuinely right.
    pub(crate) truth: String,
    /// The options on offer.
    pub(crate) options: Vec<ScenarioOption>,
    /// The members, in seating order.
    pub(crate) agents: Vec<ScenarioAgent>,
}

impl Scenario {
    /// Read a scenario from its file format.
    ///
    /// # Errors
    ///
    /// Returns a description of the first malformed or missing part.
    pub(crate) fn parse(text: &str) -> Result<Self, String> {
        let mut task = String::new();
        let mut truth = String::new();
        let mut options: Vec<ScenarioOption> = Vec::new();
        let mut agents: Vec<ScenarioAgent> = Vec::new();
        let mut section = Section::Top;

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                let (kind, id) = header
                    .split_once(char::is_whitespace)
                    .ok_or_else(|| format!("section header {header:?} names no id"))?;
                let id = id.trim().to_owned();
                match kind {
                    "option" => {
                        options.push(ScenarioOption {
                            id,
                            description: String::new(),
                        });
                        section = Section::Option;
                    }
                    "agent" => {
                        agents.push(ScenarioAgent {
                            id,
                            role: String::new(),
                            knows: Vec::new(),
                        });
                        section = Section::Agent;
                    }
                    other => return Err(format!("unknown section {other:?}")),
                }
                continue;
            }
            let (key, value) = match line.split_once(':') {
                Some((key, value)) => (key.trim(), value.trim()),
                None => ("", line),
            };
            match (&section, key) {
                (Section::Top, "task") => task = value.to_owned(),
                (Section::Top, "truth") => truth = value.to_owned(),
                (Section::Option, _) => {
                    let option = options
                        .last_mut()
                        .ok_or_else(|| "option text outside a section".to_owned())?;
                    if !option.description.is_empty() {
                        option.description.push(' ');
                    }
                    option.description.push_str(line);
                }
                (Section::Agent, "role") => {
                    agents
                        .last_mut()
                        .ok_or_else(|| "role outside a section".to_owned())?
                        .role = value.to_owned();
                }
                (Section::Agent, "knows") => {
                    agents
                        .last_mut()
                        .ok_or_else(|| "knows outside a section".to_owned())?
                        .knows
                        .push(value.to_owned());
                }
                (_, key) => return Err(format!("unexpected line {key:?}: {line:?}")),
            }
        }

        if task.is_empty() {
            return Err("scenario has no task".to_owned());
        }
        if options.len() < 2 {
            return Err("scenario needs at least two options".to_owned());
        }
        if agents.len() < 2 {
            return Err("scenario needs at least two agents".to_owned());
        }
        if !options.iter().any(|option| option.id == truth) {
            return Err(format!("truth {truth:?} is not one of the options"));
        }
        Ok(Self {
            task,
            truth,
            options,
            agents,
        })
    }

    /// The shared brief: the task, then every option under the id the room
    /// should use for it.
    ///
    /// Naming the ids up front is not decoration. Live rooms coin a fresh
    /// topic id for an option the room already has a name for, and support
    /// split across two names for one idea never adds up to a quorum.
    pub(crate) fn brief(&self) -> String {
        let mut brief = format!("{}\n\nThe options, by the id the room uses:\n", self.task);
        for option in &self.options {
            let _ = writeln!(brief, "#{} — {}", option.id, option.description);
        }
        brief
    }

    /// What only this member knows, as a block for its prompt.
    pub(crate) fn private_brief(agent: &ScenarioAgent) -> String {
        let mut brief = String::from(
            "Facts you hold that nobody else in the room holds. They are true. Nobody else can \
             use one until you have put it into the transcript:\n",
        );
        for fact in &agent.knows {
            let _ = writeln!(brief, "- {fact}");
        }
        brief
    }

    /// The member ids, in seating order.
    pub(crate) fn member_ids(&self) -> Vec<&str> {
        self.agents.iter().map(|agent| agent.id.as_str()).collect()
    }
}

/// Which section the parser is inside.
enum Section {
    /// Before any section header.
    Top,
    /// Inside an `[option ...]`.
    Option,
    /// Inside an `[agent ...]`.
    Agent,
}
