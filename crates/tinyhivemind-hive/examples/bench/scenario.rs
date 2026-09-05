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
//! truth_expert: planner
//!
//! [option rollback]
//! expert: planner
//! One sentence describing it.
//!
//! [desk payments]
//! name: Payments
//!
//! [agent planner]
//! desk: payments
//! role: release manager, who owns what can and cannot be shipped
//! expert_on: migrations
//! tier: reasoning
//! knows: a fact this member holds and nobody else does
//! ```
//!
//! Declaring more than one `[desk ...]` makes the scenario **federated**: the
//! members are split across channels that cannot see one another's
//! transcripts, and the only route between them is a referral. A federated
//! hidden profile is the sharpest form of the test — the facts that overturn
//! the decoy are not merely held by another member, they are held in another
//! room.
//!
//! Four more keys let a scenario name who the room ought to lean on and how
//! expensive each seat is meant to be. `expert:` on an option names the agent
//! it is honest to defer to on that specific call. `expert_on:` on an agent is
//! repeatable and names the *areas* it specializes in — never an option id, so
//! knowing who the specialist is does not itself give away the answer — and is
//! folded into that member's private brief with an explicit caveat that being
//! leaned on is not the same as being right. `tier: cheap` or `tier:
//! reasoning` records which model tier a seat stands in for. `truth_expert:`
//! at the top level names the agent who holds the fact that actually decides
//! the truth; that agent must hold at least one `knows:` line.

use std::fmt::Write as _;

/// One option the room may settle on.
pub(crate) struct ScenarioOption {
    /// The topic id the room uses for it, without the `#`.
    pub(crate) id: String,
    /// One sentence saying what it is.
    pub(crate) description: String,
    /// The agent id it is honest to defer to on this option, if any.
    pub(crate) expert: Option<String>,
}

/// One channel a scenario's members are split across.
pub(crate) struct ScenarioDesk {
    /// Canonical desk id, used in `@#id` and as the conversation id.
    pub(crate) id: String,
    /// Operator-facing display name.
    pub(crate) name: String,
}

/// One member of the room, and what only it knows.
pub(crate) struct ScenarioAgent {
    /// Canonical agent id, matching a desk member.
    pub(crate) id: String,
    /// The desk this member sits on, or `None` in a single-channel scenario.
    pub(crate) desk: Option<String>,
    /// What this member is on the team to do.
    pub(crate) role: String,
    /// Facts held by this member and by nobody else.
    pub(crate) knows: Vec<String>,
    /// Areas this member is the room's specialist on, named as topics rather
    /// than as option ids, so knowing who the expert is does not itself name
    /// the answer.
    pub(crate) expert_on: Vec<String>,
    /// How expensive a model this seat is meant to stand in for, if the
    /// scenario says: `cheap` or `reasoning`.
    pub(crate) tier: Option<String>,
}

/// A problem a live room is asked to decide.
pub(crate) struct Scenario {
    /// The brief every member sees, appended to the shared journal.
    pub(crate) task: String,
    /// The option that is genuinely right.
    pub(crate) truth: String,
    /// The agent id who holds the decisive private fact behind `truth`, if
    /// the scenario names one.
    pub(crate) truth_expert: Option<String>,
    /// The options on offer.
    pub(crate) options: Vec<ScenarioOption>,
    /// The channels the members are split across, if the scenario declares any.
    pub(crate) desks: Vec<ScenarioDesk>,
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
        let mut truth_expert: Option<String> = None;
        let mut options: Vec<ScenarioOption> = Vec::new();
        let mut agents: Vec<ScenarioAgent> = Vec::new();
        let mut desks: Vec<ScenarioDesk> = Vec::new();
        let mut section = Section::Top;

        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(header) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = open_section(header, &mut options, &mut desks, &mut agents)?;
                continue;
            }
            let (key, value) = match line.split_once(':') {
                Some((key, value)) => (key.trim(), value.trim()),
                None => ("", line),
            };
            match (&section, key) {
                (Section::Top, "task") => value.clone_into(&mut task),
                (Section::Top, "truth") => value.clone_into(&mut truth),
                (Section::Top, "truth_expert") => truth_expert = Some(value.to_owned()),
                (Section::Option, "expert") => {
                    options
                        .last_mut()
                        .ok_or_else(|| "expert outside a section".to_owned())?
                        .expert = Some(value.to_owned());
                }
                (Section::Option, _) => {
                    let option = options
                        .last_mut()
                        .ok_or_else(|| "option text outside a section".to_owned())?;
                    if !option.description.is_empty() {
                        option.description.push(' ');
                    }
                    option.description.push_str(line);
                }
                (Section::Desk, "name") => {
                    value.clone_into(
                        &mut desks
                            .last_mut()
                            .ok_or_else(|| "name outside a section".to_owned())?
                            .name,
                    );
                }
                (Section::Agent, "desk") => {
                    agents
                        .last_mut()
                        .ok_or_else(|| "desk outside a section".to_owned())?
                        .desk = Some(value.to_owned());
                }
                (Section::Agent, "role") => {
                    value.clone_into(
                        &mut agents
                            .last_mut()
                            .ok_or_else(|| "role outside a section".to_owned())?
                            .role,
                    );
                }
                (Section::Agent, "knows") => {
                    agents
                        .last_mut()
                        .ok_or_else(|| "knows outside a section".to_owned())?
                        .knows
                        .push(value.to_owned());
                }
                (Section::Agent, "expert_on") => {
                    agents
                        .last_mut()
                        .ok_or_else(|| "expert_on outside a section".to_owned())?
                        .expert_on
                        .push(value.to_owned());
                }
                (Section::Agent, "tier") => {
                    agents
                        .last_mut()
                        .ok_or_else(|| "tier outside a section".to_owned())?
                        .tier = Some(value.to_owned());
                }
                (_, key) => return Err(format!("unexpected line {key:?}: {line:?}")),
            }
        }

        validate(
            &task,
            &truth,
            truth_expert.as_deref(),
            &options,
            &desks,
            &agents,
        )?;
        Ok(Self {
            task,
            truth,
            truth_expert,
            options,
            desks,
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
        if !agent.expert_on.is_empty() {
            let areas = agent.expert_on.join(", ");
            let _ = writeln!(
                brief,
                "You are the room's specialist on: {areas}. That means the room may lean on \
                 you for these, and it does not mean you are right about them."
            );
        }
        brief
    }

    /// The member ids, in seating order.
    pub(crate) fn member_ids(&self) -> Vec<&str> {
        self.agents.iter().map(|agent| agent.id.as_str()).collect()
    }

    /// The channels this scenario declares, with their members.
    ///
    /// A scenario that declares no desk is one room, which is the shape the
    /// single-desk live arm already runs.
    pub(crate) fn channels(&self) -> Vec<crate::swarm::Channel> {
        self.desks
            .iter()
            .map(|desk| crate::swarm::Channel {
                id: desk.id.clone(),
                name: desk.name.clone(),
                members: self
                    .agents
                    .iter()
                    .filter(|agent| agent.desk.as_deref() == Some(desk.id.as_str()))
                    .map(|agent| agent.id.clone())
                    .collect(),
            })
            .collect()
    }
}

/// Check the parsed pieces of a scenario against each other, once the file
/// has been read in full.
///
/// # Errors
///
/// Returns a description of the first inconsistency found.
fn validate(
    task: &str,
    truth: &str,
    truth_expert: Option<&str>,
    options: &[ScenarioOption],
    desks: &[ScenarioDesk],
    agents: &[ScenarioAgent],
) -> Result<(), String> {
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
    for agent in agents {
        if let Some(desk) = &agent.desk
            && !desks.iter().any(|record| record.id == *desk)
        {
            return Err(format!(
                "agent {:?} sits on unknown desk {desk:?}",
                agent.id
            ));
        }
        if let Some(tier) = &agent.tier
            && tier != "cheap"
            && tier != "reasoning"
        {
            return Err(format!(
                "agent {:?} names unknown tier {tier:?}, want cheap or reasoning",
                agent.id
            ));
        }
    }
    if desks.len() > 1
        && let Some(loose) = agents.iter().find(|agent| agent.desk.is_none())
    {
        return Err(format!(
            "agent {:?} names no desk, and a federated scenario has more than one",
            loose.id,
        ));
    }
    for option in options {
        if let Some(expert) = &option.expert
            && !agents.iter().any(|agent| agent.id == *expert)
        {
            return Err(format!(
                "option {:?} names unknown expert {expert:?}",
                option.id
            ));
        }
    }
    for agent in agents {
        if let Some(area) = agent
            .expert_on
            .iter()
            .find(|area| options.iter().any(|option| option.id == **area))
        {
            return Err(format!(
                "agent {:?} names option id {area:?} as an expert_on area, which exposes that \
                 option id in its private brief",
                agent.id
            ));
        }
    }
    if let Some(expert) = truth_expert {
        let member = agents
            .iter()
            .find(|agent| agent.id == expert)
            .ok_or_else(|| format!("truth_expert names unknown agent {expert:?}"))?;
        if member.knows.is_empty() {
            return Err("truth_expert names a member holding no private facts".to_owned());
        }
    }
    Ok(())
}

/// Open the section a `[kind id]` header names, and return which it is.
fn open_section(
    header: &str,
    options: &mut Vec<ScenarioOption>,
    desks: &mut Vec<ScenarioDesk>,
    agents: &mut Vec<ScenarioAgent>,
) -> Result<Section, String> {
    let (kind, id) = header
        .split_once(char::is_whitespace)
        .ok_or_else(|| format!("section header {header:?} names no id"))?;
    let id = id.trim().to_owned();
    match kind {
        "option" => {
            options.push(ScenarioOption {
                id,
                description: String::new(),
                expert: None,
            });
            Ok(Section::Option)
        }
        "desk" => {
            desks.push(ScenarioDesk {
                name: id.clone(),
                id,
            });
            Ok(Section::Desk)
        }
        "agent" => {
            agents.push(ScenarioAgent {
                id,
                desk: None,
                role: String::new(),
                knows: Vec::new(),
                expert_on: Vec::new(),
                tier: None,
            });
            Ok(Section::Agent)
        }
        other => Err(format!("unknown section {other:?}")),
    }
}

/// Which section the parser is inside.
enum Section {
    /// Before any section header.
    Top,
    /// Inside an `[option ...]`.
    Option,
    /// Inside a `[desk ...]`.
    Desk,
    /// Inside an `[agent ...]`.
    Agent,
}
