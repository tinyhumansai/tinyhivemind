//! Several channels solving one problem, and the messages that cross between
//! them.
//!
//! [`crate::run`] drives one episode on one desk. This drives several at once:
//! one journal per desk, one episode per desk, and a scheduler that lets a
//! member spend its turn asking *another channel* a question instead of
//! arguing in its own. The asking, the routing and the answer's way home are
//! the library's `referral` fold; everything else here is the host side a
//! consumer would write.
//!
//! # What crosses, and what does not
//!
//! A referral carries **information**, never a vote. The message that lands on
//! the far desk is `!evidence`, which adds no supporter to any topic: the
//! members of that desk hear another channel's reading of an option, average
//! it into their own, and then have to spend their own turns saying so before
//! anything is counted. A mechanism that let one desk's support count on
//! another desk would not be pooling information, it would be voting twice.
//!
//! # The accounting
//!
//! Every arm is charged for every agent invocation, including the ones a
//! referral causes on the far desk and on the way back. A member that spends
//! its turn asking another desk does not also get to argue in its own that
//! turn. Without that the swarm arm would simply be the siloed arm with extra
//! compute, and the comparison would mean nothing.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::time::{Duration, Instant};

use tinyhivemind_hive::{
    Conversation, EpisodePolicy, EpisodeState, HiveStep, HiveTurn, Sequence, SessionAuthor,
    SessionMessage,
    desk::{Desk, DeskSet, ResponderMode},
    dispatch::{DispatchConversation, DispatchKey},
    mention::{MentionAuthor, resolve as resolve_mentions},
    project_for,
    referral::{
        Referral, ReferralDecision, ReferralInput, ReferralKind, ReferralOrigin, ReferralPolicy,
        referral,
    },
    roster::{Roster, RosterMember},
    step,
    trace::TopicId,
};

use crate::federation::Federation;
use crate::run::Ending;

/// One channel: a desk id, its display name, and who sits on it.
#[derive(Clone, Debug)]
pub(crate) struct Channel {
    /// Canonical desk id, and the conversation its episode runs on.
    pub(crate) id: String,
    /// Operator-facing display name.
    pub(crate) name: String,
    /// Member ids, in seating order.
    pub(crate) members: Vec<String>,
}

/// Anything that can fill a turn on a desk in a federation.
///
/// Simulated members and real agent processes differ only here. In particular
/// the scheduler never learns which it is holding: an ask that crosses a
/// channel is a line of text either way, and it is routed by the same mention
/// grammar and the same `referral` fold whether arithmetic or a language model
/// wrote it.
pub(crate) trait SwarmMember {
    /// Canonical agent id, matching a desk member.
    fn id(&self) -> &str;

    /// Offer to spend this turn asking one of `peers` instead of arguing here.
    ///
    /// A member that would rather write its own line — which is every real
    /// agent, since a real agent decides that inside its own turn — returns
    /// `None` and the scheduler reads whatever mention its reply carries.
    fn ask(&mut self, peers: &[&str]) -> Option<String> {
        let _ = peers;
        None
    }

    /// Fill one authorized episode turn.
    ///
    /// # Errors
    ///
    /// Returns a host-side failure, such as an agent process that did not
    /// answer.
    fn speak(&mut self, turn: &HiveTurn, visible: &[&SessionMessage]) -> Result<String, String>;

    /// Fill one turn caused by a message that arrived from another channel.
    ///
    /// # Errors
    ///
    /// Returns a host-side failure, such as an agent process that did not
    /// answer.
    fn answer(
        &mut self,
        incoming: &Referral,
        visible: &[&SessionMessage],
    ) -> Result<String, String>;

    /// Take in whatever a message just appended to this desk carries.
    ///
    /// Every member of a desk is offered every line written on it, which is
    /// what makes one member's question worth a turn for the whole desk.
    fn absorb(&mut self, content: &str) {
        let _ = content;
    }
}

/// What one federation-wide run decided, and what it spent.
#[derive(Clone, Debug, Default)]
pub(crate) struct SwarmReport {
    /// The option the federation settled on, by plurality of its desks.
    pub(crate) decided: Option<TopicId>,
    /// Whether that option is the genuinely best one.
    pub(crate) correct: bool,
    /// How each desk ended, in snapshot order.
    pub(crate) desks: Vec<DeskOutcome>,
    /// Agent invocations across every desk, including referred turns.
    pub(crate) turns: u32,
    /// Referrals that left the desk that made them.
    pub(crate) crossings: u32,
    /// Answers that arrived after the desk that asked had already finished.
    pub(crate) stranded: u32,
    /// Calls into [`step`], including the terminal ones.
    pub(crate) step_calls: u32,
    /// Time spent inside the library.
    pub(crate) library_time: Duration,
    /// One line per message, for the trace view.
    pub(crate) trace: Vec<String>,
}

/// How one desk's own episode ended.
#[derive(Clone, Debug)]
pub(crate) struct DeskOutcome {
    /// The desk's display name.
    pub(crate) name: String,
    /// How its episode ended.
    pub(crate) ending: Ending,
    /// What it settled on, if it settled.
    pub(crate) decided: Option<TopicId>,
}

/// A host owning one append-only journal per desk.
struct SwarmHost {
    journals: Vec<Vec<SessionMessage>>,
    members: Vec<RosterMember>,
    desks: Vec<Desk>,
    retired: Vec<String>,
}

impl SwarmHost {
    fn new(channels: &[Channel]) -> Self {
        Self {
            journals: vec![Vec::new(); channels.len()],
            members: channels
                .iter()
                .flat_map(|channel| channel.members.iter())
                .map(|id| RosterMember {
                    id: id.clone(),
                    name: Some(id.clone()),
                })
                .collect(),
            desks: channels
                .iter()
                .map(|channel| Desk {
                    id: channel.id.clone(),
                    name: channel.name.clone(),
                    description: Some(format!("The {} desk", channel.name)),
                    members: channel.members.clone(),
                    responder_mode: ResponderMode::Auto,
                })
                .collect(),
            retired: Vec::new(),
        }
    }

    fn roster(&self) -> Roster<'_> {
        Roster::new(&self.members, &[], &self.retired)
    }

    fn desks(&self) -> DeskSet<'_> {
        DeskSet::new(&self.desks, &[], &[], &[], &self.retired)
    }

    fn conversation(&self, desk: usize) -> Conversation {
        let record = &self.desks[desk];
        Conversation {
            desk_id: record.id.clone(),
            desk_name: record.name.clone(),
            thread_root: None,
        }
    }

    /// The sequence the next message on a desk will take.
    ///
    /// Sequences are numbered per conversation, exactly as a host that stores
    /// one channel per journal would number them, so a citation `^N` names the
    /// same message on the desk that reads it.
    fn next_sequence(&self, desk: usize) -> Sequence {
        Sequence(
            u64::try_from(self.journals[desk].len())
                .unwrap_or(u64::MAX)
                .saturating_add(1),
        )
    }

    /// The sequence of the last message on a desk.
    fn watermark(&self, desk: usize) -> Sequence {
        self.journals[desk]
            .last()
            .map_or(Sequence(0), |message| message.sequence)
    }

    fn append(&mut self, desk: usize, author: SessionAuthor, content: String) -> Sequence {
        let sequence = self.next_sequence(desk);
        self.journals[desk].push(SessionMessage {
            sequence,
            author,
            content,
        });
        sequence
    }

    fn operator(&mut self, desk: usize, content: &str) {
        self.append(desk, SessionAuthor::Operator, content.to_owned());
    }

    fn agent(&mut self, desk: usize, id: &str, content: String) -> Sequence {
        self.append(
            desk,
            SessionAuthor::Agent {
                id: id.to_owned(),
                label: id.to_owned(),
            },
            content,
        )
    }

    fn index_of(&self, desk_id: &str) -> Option<usize> {
        self.desks.iter().position(|desk| desk.id == desk_id)
    }
}

/// Run one episode per channel, letting members cross between them when the
/// policy allows it.
///
/// With `referrals.enabled` false this is the siloed control: the same desks,
/// the same members, the same budgets, and no way to reach another channel.
///
/// The loop is the whole host contract, twice over. Per desk it is the
/// familiar one — fold, run the single turn the library authorized, append it,
/// commit the returned state. Around that it adds the only thing a federation
/// needs: after every appended reply, ask `referral` whether one child turn is
/// owed somewhere else, and if it is, queue it for the channel it named.
///
/// # Errors
///
/// Returns the library's own error text for a malformed snapshot, or a
/// member's own failure.
pub(crate) fn drive_swarm(
    channels: &[Channel],
    members: &mut [&mut dyn SwarmMember],
    policy: &EpisodePolicy,
    referrals: ReferralPolicy,
    task: &str,
    keep_trace: bool,
) -> Result<SwarmReport, String> {
    let count = channels.len();
    let mut board = Board {
        host: SwarmHost::new(channels),
        channels,
        referrals,
        keep_trace,
        pending: vec![VecDeque::new(); count],
        // Asks are counted per desk rather than per member, and capped at one
        // per peer channel. The answer lands in the desk's own transcript,
        // where every member of it reads the same line — which is what a shared
        // medium is for, and what makes a second member asking the same desk
        // the same question pure cost. `referral` bounds how *deep* a chain
        // goes; bounding how *wide* one desk may go is the host's job, and this
        // is it.
        asks: vec![0; count],
        report: SwarmReport::default(),
    };
    for desk in 0..count {
        board.host.operator(desk, task);
    }

    let mut states: Vec<EpisodeState> = (0..count)
        .map(|desk| EpisodeState::opened(board.host.conversation(desk), board.host.watermark(desk)))
        .collect();
    let mut finished: Vec<Option<DeskOutcome>> = vec![None; count];

    loop {
        let mut progressed = false;
        for desk in 0..count {
            if finished[desk].is_some() {
                board.strand(desk);
                continue;
            }
            if let Some(incoming) = board.pending[desk].pop_front() {
                board.deliver(members, desk, &incoming)?;
                progressed = true;
                continue;
            }

            let started = Instant::now();
            let decision = {
                let roster = board.host.roster();
                let desk_set = board.host.desks();
                step(
                    &states[desk],
                    &board.host.journals[desk],
                    &roster,
                    &desk_set,
                    policy,
                )
            };
            board.report.library_time += started.elapsed();
            board.report.step_calls = board.report.step_calls.saturating_add(1);

            match decision.map_err(|error| error.to_string())? {
                HiveStep::Speak { turn } => {
                    let turn = *turn;
                    board.take_turn(members, desk, &turn)?;
                    states[desk] = turn.next_state;
                }
                HiveStep::Converged { topic, .. } => {
                    finished[desk] = Some(outcome(channels, desk, Ending::Converged, Some(topic)));
                }
                HiveStep::Deadlocked { .. } => {
                    finished[desk] = Some(outcome(channels, desk, Ending::Deadlocked, None));
                }
                HiveStep::Exhausted { .. } => {
                    finished[desk] = Some(outcome(channels, desk, Ending::Exhausted, None));
                }
                HiveStep::Idle => {
                    finished[desk] = Some(outcome(channels, desk, Ending::Idle, None));
                }
            }
            progressed = true;
        }
        if !progressed {
            break;
        }
    }

    let mut report = board.report;
    report.desks = finished.into_iter().flatten().collect();
    report.decided = plurality(&report.desks);
    Ok(report)
}

/// The scheduler's own state: the journals, what is in flight, and the tally.
struct Board<'a> {
    host: SwarmHost,
    channels: &'a [Channel],
    referrals: ReferralPolicy,
    keep_trace: bool,
    /// Referrals routed to each desk and not yet run.
    pending: Vec<VecDeque<Referral>>,
    /// Peer channels each desk has already asked.
    asks: Vec<usize>,
    report: SwarmReport,
}

impl Board<'_> {
    /// Give up on everything routed to a desk that has already finished.
    ///
    /// An answer that arrives after the desk that asked has closed is
    /// information the federation paid for and cannot use. It is counted rather
    /// than quietly dropped.
    fn strand(&mut self, desk: usize) {
        self.report.stranded = self
            .report
            .stranded
            .saturating_add(u32::try_from(self.pending[desk].len()).unwrap_or(0));
        self.pending[desk].clear();
    }

    /// Run one turn caused by a message that arrived from another channel.
    fn deliver(
        &mut self,
        members: &mut [&mut dyn SwarmMember],
        desk: usize,
        incoming: &Referral,
    ) -> Result<(), String> {
        let seat = seat_of(members, &incoming.target_id)?;
        let content = {
            let visible: Vec<&SessionMessage> = self.host.journals[desk].iter().collect();
            members[seat].answer(incoming, &visible)?
        };
        let sequence = self.commit(members, desk, &incoming.target_id, &content);
        // Consider the back edge. A reply committed under a crossing referral,
        // carrying no mention of its own, is exactly the case `referral`
        // answers with one `Return`.
        self.route(
            desk,
            &incoming.target_id,
            &content,
            sequence,
            incoming.child_hop,
            incoming.origin.clone(),
        )?;
        Ok(())
    }

    /// Run one turn the episode authorized, which the member may spend asking.
    fn take_turn(
        &mut self,
        members: &mut [&mut dyn SwarmMember],
        desk: usize,
        turn: &HiveTurn,
    ) -> Result<(), String> {
        let seat = seat_of(members, &turn.agent_id)?;
        let peers: Vec<&str> = self
            .channels
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != desk)
            .map(|(_, channel)| channel.id.as_str())
            .skip(self.asks[desk])
            .collect();
        let budget =
            self.referrals.enabled && self.asks[desk] < self.channels.len().saturating_sub(1);
        let mut offered = false;
        let content = {
            let visible = project_for(turn, &self.host.journals[desk]);
            let ask = if budget {
                members[seat].ask(&peers)
            } else {
                None
            };
            match ask {
                Some(body) => {
                    offered = true;
                    body
                }
                None => members[seat].speak(turn, &visible)?,
            }
        };
        let sequence = self.commit(members, desk, &turn.agent_id, &content);
        let routed = budget && self.route(desk, &turn.agent_id, &content, sequence, 0, None)?;
        // A turn spent asking is spent whether or not the question found its
        // way out, so the budget is charged either way.
        if offered || routed {
            self.asks[desk] = self.asks[desk].saturating_add(1);
        }
        Ok(())
    }

    /// Append one reply, offer it to everybody on the desk, and count the turn.
    fn commit(
        &mut self,
        members: &mut [&mut dyn SwarmMember],
        desk: usize,
        agent_id: &str,
        content: &str,
    ) -> Sequence {
        let sequence = self.host.agent(desk, agent_id, content.to_owned());
        for member in &self.channels[desk].members {
            if let Ok(seat) = seat_of(members, member) {
                members[seat].absorb(content);
            }
        }
        self.report.turns = self.report.turns.saturating_add(1);
        if self.keep_trace {
            self.report
                .trace
                .push(line(self.channels, desk, sequence, agent_id, content));
        }
        sequence
    }

    /// Ask `referral` whether this reply owes one turn to another channel.
    ///
    /// Returns whether one was queued. The mention is read out of the authored
    /// text by the real grammar and the routing decision is the real fold:
    /// nothing here shortcuts either, and nothing here knows whether a model or
    /// a table of numbers wrote the line.
    fn route(
        &mut self,
        desk: usize,
        author_id: &str,
        content: &str,
        sequence: Sequence,
        hop: u32,
        origin: Option<ReferralOrigin>,
    ) -> Result<bool, String> {
        if !self.referrals.enabled {
            return Ok(false);
        }
        let one = {
            let roster = self.host.roster();
            let desk_set = self.host.desks();
            let mentions = resolve_mentions(
                content,
                None,
                &MentionAuthor::Agent {
                    id: author_id.to_owned(),
                },
                &roster,
                &desk_set,
            );
            let input = ReferralInput {
                key: DispatchKey {
                    trigger_sequence: sequence.0,
                },
                conversation: DispatchConversation {
                    desk_id: self.channels[desk].id.clone(),
                    thread_root: None,
                },
                author_id: author_id.to_owned(),
                content: content.to_owned(),
                mentions,
                hop,
                origin,
            };
            match referral(self.referrals, &input, &roster, &desk_set)
                .map_err(|error| error.to_string())?
            {
                ReferralDecision::One { referral: one } => *one,
                ReferralDecision::None { .. } => return Ok(false),
            }
        };
        // A referral that would not have left the desk changes nothing here:
        // the reply is already appended where the target can read it.
        if !one.crosses() {
            return Ok(false);
        }
        let Some(target) = self.host.index_of(&one.to.desk_id) else {
            return Err(format!("referral to unknown desk {}", one.to.desk_id));
        };
        self.report.crossings = self.report.crossings.saturating_add(1);
        self.pending[target].push_back(one);
        Ok(true)
    }
}

/// Find a member by id.
fn seat_of(members: &[&mut dyn SwarmMember], agent_id: &str) -> Result<usize, String> {
    members
        .iter()
        .position(|member| member.id() == agent_id)
        .ok_or_else(|| format!("no member named {agent_id}"))
}

/// Run every desk of a simulated federation.
///
/// # Errors
///
/// Returns the library's own error text for a malformed snapshot.
pub(crate) fn run_swarm(
    federation: &Federation,
    policy: &EpisodePolicy,
    referrals: ReferralPolicy,
    task: &str,
    keep_trace: bool,
) -> Result<SwarmReport, String> {
    let channels = channels(federation);
    let mut simulated: Vec<SwarmSim> = federation
        .agents
        .iter()
        .map(|agent| {
            let mut agent = agent.clone();
            agent.set_quorum(policy.quorum);
            SwarmSim::new(federation, agent)
        })
        .collect();
    let mut members: Vec<&mut dyn SwarmMember> = simulated
        .iter_mut()
        .map(|member| member as &mut dyn SwarmMember)
        .collect();
    let report = drive_swarm(&channels, &mut members, policy, referrals, task, keep_trace)?;
    Ok(SwarmReport {
        correct: report.decided.as_ref() == Some(&federation.truth),
        ..report
    })
}

/// The channels a federation's desks describe.
pub(crate) fn channels(federation: &Federation) -> Vec<Channel> {
    federation
        .desks
        .iter()
        .map(|desk| Channel {
            id: desk.id.clone(),
            name: desk.name.clone(),
            members: desk.members.clone(),
        })
        .collect()
}

/// A simulated member of a federation.
///
/// It is a [`crate::sim::SimAgent`] — the same arithmetic participant the
/// single-desk benchmark uses, unchanged — plus the one thing a federation
/// adds: it knows which channel it is on, and it will spend a turn asking
/// another one.
pub(crate) struct SwarmSim {
    agent: crate::sim::SimAgent,
    /// This member's own desk, by display name.
    here: String,
    /// Every desk in the federation, id and display name.
    directory: Vec<(String, String)>,
    /// The options, so a reading can cover the slate rather than one option.
    topics: Vec<TopicId>,
}

impl SwarmSim {
    /// Seat one simulated agent in its federation.
    fn new(federation: &Federation, agent: crate::sim::SimAgent) -> Self {
        let here = federation
            .desk_of(&agent.id)
            .map_or_else(String::new, |desk| desk.name.clone());
        Self {
            agent,
            here,
            directory: federation
                .desks
                .iter()
                .map(|desk| (desk.id.clone(), desk.name.clone()))
                .collect(),
            topics: federation.topics.clone(),
        }
    }

    /// How this member reads every option, written the way a member would.
    fn slate(&self) -> String {
        let mut line = format!("{} reads", self.here);
        for (at, topic) in self.topics.iter().enumerate() {
            let separator = if at == 0 { "" } else { "," };
            let _ = write!(line, "{separator} #{topic} at {}", self.agent.score(topic));
        }
        line.push('.');
        line
    }

    /// A desk's display name.
    fn desk_name<'a>(&'a self, desk_id: &'a str) -> &'a str {
        self.directory
            .iter()
            .find(|(id, _)| id == desk_id)
            .map_or(desk_id, |(_, name)| name.as_str())
    }
}

impl SwarmMember for SwarmSim {
    fn id(&self) -> &str {
        &self.agent.id
    }

    /// Ask another channel before backing anything here.
    ///
    /// The rule is one a member could defend out loud: *I will not put an
    /// option on the floor on the strength of what my own desk thinks, when
    /// nobody outside this desk has told me anything about it.* The question
    /// goes out **before** the desk has backed anything, and that turns out to
    /// be the whole ballgame. A desk whose members share a bias reaches quorum
    /// inside its own blind opening round, and an answer arriving after that is
    /// information the desk has already voted past. An earlier version of this
    /// harness asked after proposing, and every desk committed to its own decoy
    /// with the correction sitting three lines below the decision.
    fn ask(&mut self, peers: &[&str]) -> Option<String> {
        let peer = peers.first()?;
        Some(format!(
            "@#{peer} We are about to back #{} here. {} How do you read them?",
            self.agent.favourite(),
            self.slate(),
        ))
    }

    fn speak(&mut self, turn: &HiveTurn, visible: &[&SessionMessage]) -> Result<String, String> {
        crate::run::Participant::speak(&mut self.agent, turn, visible)
    }

    /// Answer a question that arrived from another channel.
    ///
    /// The answer covers the whole slate rather than the one option asked
    /// about. That is what a real cross-team answer looks like, and it is also
    /// what makes the pooling work: an outside reading of one option halves
    /// that option's bias and leaves every other option untouched, which moves
    /// the argmax around without improving it.
    fn answer(
        &mut self,
        incoming: &Referral,
        _visible: &[&SessionMessage],
    ) -> Result<String, String> {
        let carried = readings(&incoming.content);
        if carried.is_empty() {
            return Ok("!question I cannot read a rating out of that.".to_owned());
        }
        Ok(match incoming.kind {
            // Repeat what was asked and add this desk's own reading, so the
            // whole exchange is legible to everybody here rather than only to
            // the two agents in it.
            ReferralKind::Forward => {
                format!("!evidence {} {}", restate(&carried), self.slate())
            }
            // Carrying an answer home: only the far desk's readings are news.
            ReferralKind::Return => {
                let from = self.desk_name(&incoming.from.desk_id).to_owned();
                let theirs: Vec<Reading> = carried
                    .into_iter()
                    .filter(|reading| reading.desk == from)
                    .collect();
                if theirs.is_empty() {
                    return Ok("!question That answer carried no rating I can use.".to_owned());
                }
                format!("!evidence {}", restate(&theirs))
            }
        })
    }

    /// Fold every outside reading a line carries into this member's own view.
    ///
    /// This is the step a channel boundary otherwise prevents, and it is
    /// deliberately the *only* thing crossing one has ever done here: a reading
    /// changes what a member believes, and the member still has to spend a turn
    /// saying so before the room counts it.
    fn absorb(&mut self, content: &str) {
        for reading in readings(content) {
            if reading.desk == self.here {
                continue;
            }
            if !self.directory.iter().any(|(_, name)| *name == reading.desk) {
                continue;
            }
            self.agent.import(&reading.topic, reading.value);
        }
    }
}

/// Hand every desk every other desk's readings for free, before anybody speaks.
///
/// This is the ceiling control, and it is the one that keeps the swarm arm
/// honest. The swarm's members exchange numeric readings, which the siloed
/// members never get the chance to; a reader is entitled to ask how much of the
/// difference is the *protocol* and how much is simply having the numbers. This
/// arm answers that: the same numbers arrive, at no turn cost, with no
/// referral, no mention and no channel crossed. Whatever it scores is what the
/// information is worth; whatever the swarm scores below it is what the channel
/// boundary still costs after `referral` has done its work.
pub(crate) fn pooled(federation: &Federation) -> Federation {
    let mut pooled = federation.clone();
    // Read every desk's slate off the desk it belongs to *before* any import,
    // so no desk's contribution is contaminated by another's.
    let slates: Vec<Vec<(TopicId, i32)>> = federation
        .desks
        .iter()
        .map(|desk| {
            let seat = desk
                .members
                .first()
                .and_then(|member| federation.seat_of(member));
            federation
                .topics
                .iter()
                .map(|topic| {
                    let value = seat.map_or(0, |seat| federation.agents[seat].score(topic));
                    (topic.clone(), value)
                })
                .collect()
        })
        .collect();
    for (index, desk) in federation.desks.iter().enumerate() {
        for member in &desk.members {
            let Some(seat) = federation.seat_of(member) else {
                continue;
            };
            for (from, slate) in slates.iter().enumerate() {
                if from == index {
                    continue;
                }
                for (topic, value) in slate {
                    pooled.agents[seat].import(topic, *value);
                }
            }
        }
    }
    pooled
}

/// Record how one desk ended.
fn outcome(
    channels: &[Channel],
    desk: usize,
    ending: Ending,
    decided: Option<TopicId>,
) -> DeskOutcome {
    DeskOutcome {
        name: channels[desk].name.clone(),
        ending,
        decided,
    }
}

/// The option most desks settled on, or `None` when they are tied.
///
/// A tie is not a decision. Breaking it by desk order would hand the federation
/// a win it did not earn, which is the kind of quiet thumb on the scale a
/// benchmark exists to avoid.
fn plurality(desks: &[DeskOutcome]) -> Option<TopicId> {
    let mut tally: Vec<(&TopicId, u32)> = Vec::new();
    for decided in desks.iter().filter_map(|desk| desk.decided.as_ref()) {
        match tally.iter_mut().find(|(topic, _)| *topic == decided) {
            Some(entry) => entry.1 = entry.1.saturating_add(1),
            None => tally.push((decided, 1)),
        }
    }
    let most = tally.iter().map(|(_, count)| *count).max()?;
    let mut leaders = tally.iter().filter(|(_, count)| *count == most);
    let leader = leaders.next()?;
    if leaders.next().is_some() {
        return None;
    }
    Some(leader.0.clone())
}

/// One desk's reading of one option, as it appears in a message.
#[derive(Clone, Debug)]
struct Reading {
    /// The desk that holds the reading, by display name.
    desk: String,
    /// The option it is a reading of.
    topic: TopicId,
    /// What that desk rates it.
    value: i32,
}

/// Write a set of readings back out in the form they were read from.
fn restate(readings: &[Reading]) -> String {
    let mut line = String::new();
    let mut current: Option<&str> = None;
    for reading in readings {
        if current.is_none_or(|desk| desk != reading.desk) {
            if current.is_some() {
                line.push_str(". ");
            }
            let _ = write!(line, "{} reads", reading.desk);
            current = Some(reading.desk.as_str());
        } else {
            line.push(',');
        }
        let _ = write!(line, " #{} at {}", reading.topic, reading.value);
    }
    if current.is_some() {
        line.push('.');
    }
    line
}

/// Read every `<Desk> reads #option at <n>, #option at <n>.` clause out of a
/// message.
///
/// The form is the one the members write, and it is meant to survive a real
/// agent writing it in a sentence of its own rather than as a field: a desk
/// name followed by `reads` opens a clause, and every `#option at <n>` until
/// the next desk name belongs to it.
fn readings(content: &str) -> Vec<Reading> {
    let words: Vec<&str> = content.split_whitespace().collect();
    let mut found = Vec::new();
    let mut desk: Option<String> = None;
    for (at, word) in words.iter().enumerate() {
        if *word == "reads"
            && let Some(name) = at.checked_sub(1).and_then(|before| words.get(before))
        {
            desk = Some((*name).to_owned());
            continue;
        }
        let Some(topic) = word.strip_prefix('#') else {
            continue;
        };
        let topic = topic.trim_end_matches(['.', ',', ';', '?', '!']);
        if topic.is_empty() || words.get(at + 1) != Some(&"at") {
            continue;
        }
        let Some(value) = words.get(at + 2) else {
            continue;
        };
        let Ok(value) = value.trim_end_matches(['.', ',', ';']).parse::<i32>() else {
            continue;
        };
        let Some(desk) = desk.clone() else {
            continue;
        };
        found.push(Reading {
            desk,
            topic: TopicId::from(topic),
            value,
        });
    }
    found
}

/// One transcript line, tagged with the channel it was written in.
fn line(
    channels: &[Channel],
    desk: usize,
    sequence: Sequence,
    agent_id: &str,
    content: &str,
) -> String {
    format!(
        "{:>9} {:>3}  {:<18} {content}",
        channels[desk].name, sequence.0, agent_id,
    )
}
