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
//!
//! # Layout
//!
//! This module holds the desks-and-journals host (`SwarmHost`) and the two
//! entry points a caller drives (`drive_swarm` and `run_swarm`). The
//! scheduler that owns one run's in-flight referrals lives in the `board`
//! submodule; the simulated participant that can ask another channel lives
//! in `member`; the wire text those participants read and write lives in
//! `format`.

use std::time::Duration;

use tinyhivemind_hive::{
    Conversation, EpisodePolicy, EpisodeState, HiveTurn, Sequence, SessionAuthor,
    SessionMessage,
    desk::{Desk, DeskSet, ResponderMode},
    referral::{Referral, ReferralPolicy},
    roster::{Roster, RosterMember},
    trace::TopicId,
};

use crate::federation::Federation;
use crate::run::Ending;
use tinyhivemind_hive::aside::Audience;

pub(crate) use board::AskChannel;

use board::Board;
use member::SwarmSim;

mod board;
mod format;
mod member;
mod schedule;

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
pub(crate) trait SwarmMember: Send {
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
    fn speak(&mut self, turn: &HiveTurn, visible: &[SessionMessage]) -> Result<String, String>;

    /// Fill one turn caused by a message that arrived from another channel.
    ///
    /// # Errors
    ///
    /// Returns a host-side failure, such as an agent process that did not
    /// answer.
    fn answer(&mut self, incoming: &Referral, visible: &[SessionMessage])
    -> Result<String, String>;

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
    /// Questions put to another channel **off the floor**, taking no turn.
    ///
    /// Priced in a column of its own rather than folded into `turns`, on the
    /// same principle the off-floor exchange's `calls/ep` follows: it is a
    /// model call the federation paid for and the turn count does not show.
    pub(crate) off_floor_asks: u32,
    /// Answers that arrived after the desk that asked had already finished.
    pub(crate) stranded: u32,
    /// Turns, across every desk, whose content is a `!defer` line.
    pub(crate) defers: u32,
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
    /// One transcript per desk, indexed the same as `desks`.
    journals: Vec<Vec<SessionMessage>>,
    /// Every member across every desk, for the roster the library reads.
    members: Vec<RosterMember>,
    /// The desks themselves, in the order the caller listed their channels.
    desks: Vec<Desk>,
    /// Desks that have left the federation. Always empty here: a bench run
    /// never retires a desk mid-run, but the library's `DeskSet` still needs
    /// the slice to build one.
    retired: Vec<String>,
}

impl SwarmHost {
    /// Build a host with one empty journal per channel and a desk/roster to
    /// match.
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

    /// The roster the library reads, spanning every desk in the federation.
    fn roster(&self) -> Roster<'_> {
        Roster::new(&self.members, &[], &self.retired)
    }

    /// The desk set the library reads, spanning every desk in the federation.
    fn desks(&self) -> DeskSet<'_> {
        DeskSet::new(&self.desks, &[], &[], &[], &self.retired)
    }

    /// The conversation one desk's episode runs on.
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

    /// Append one message to a desk's journal and return its sequence.
    fn append(&mut self, desk: usize, author: SessionAuthor, content: String) -> Sequence {
        let sequence = self.next_sequence(desk);
        self.journals[desk].push(SessionMessage {
            sequence,
            author,
            content,
            audience: Audience::Desk,
            elided: None,
        });
        sequence
    }

    /// Append the task line that opens a desk's episode.
    fn operator(&mut self, desk: usize, content: &str) {
        self.append(desk, SessionAuthor::Operator, content.to_owned());
    }

    /// Append one agent's reply to a desk's journal.
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

    /// The position of a desk by id, if the federation has one.
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
    members: &mut [Vec<&mut dyn SwarmMember>],
    policy: &EpisodePolicy,
    referrals: ReferralPolicy,
    asking: AskChannel,
    jobs: usize,
    task: &str,
    keep_trace: bool,
) -> Result<SwarmReport, String> {
    let count = channels.len();
    let mut board = Board::new(channels, referrals, keep_trace, asking);
    for desk in 0..count {
        board.host_mut().operator(desk, task);
    }

    let mut states: Vec<EpisodeState> = (0..count)
        .map(|desk| {
            EpisodeState::opened(
                board.host().conversation(desk),
                board.host().watermark(desk),
            )
        })
        .collect();
    let mut finished: Vec<Option<DeskOutcome>> = vec![None; count];

    loop {
        // Two schedulers, and the choice between them is not a performance
        // detail. See `schedule.rs`: they interleave a routed referral
        // differently, so they can decide differently, and the sequential one
        // is the reference every recorded swarm number was taken against.
        let progressed = if jobs > 1 {
            schedule::concurrent_pass(
                &mut board,
                members,
                channels,
                &mut states,
                &mut finished,
                policy,
                jobs,
            )?
        } else {
            schedule::sequential_pass(
                &mut board,
                members,
                channels,
                &mut states,
                &mut finished,
                policy,
            )?
        };
        if !progressed {
            break;
        }
    }

    let mut report = board.into_report();
    report.desks = finished.into_iter().flatten().collect();
    report.decided = member::plurality(&report.desks);
    Ok(report)
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
    asking: AskChannel,
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
    // Grouped by desk, in seating order, which is how the scheduler wants
    // them: every member access a desk makes is to its own seats, so grouping
    // turns a lookup across the whole federation into one across a desk.
    let mut members = group_by_desk(&channels, simulated.iter_mut().map(|member| member as _));
    // One job, deliberately. A simulated turn is arithmetic, so there is
    // nothing here worth a thread — and the caller is already running whole
    // federations in parallel, so spawning again inside each one would
    // oversubscribe the machine rather than speed anything up. The concurrent
    // path exists for live desks, where a turn is a model call.
    let report = drive_swarm(
        &channels,
        &mut members,
        policy,
        referrals,
        asking,
        1,
        task,
        keep_trace,
    )?;
    Ok(SwarmReport {
        correct: report.decided.as_ref() == Some(&federation.truth),
        ..report
    })
}

/// Group a federation's members by the desk they sit on, in seating order.
///
/// The scheduler only ever reaches a member through the desk it sits on — a
/// turn, an answer to a referral, an off-floor ask, and the line every member
/// of a desk absorbs are all desk-local — so holding the seats grouped is both
/// the shape the work has and the shape that makes it cheap: a lookup inside
/// one desk rather than a scan across every member of every desk.
///
/// It is also what lets the desks run at the same time. Each desk's seats are
/// a distinct `Vec`, so a mutable borrow of one desk's seats is disjoint from
/// every other desk's, and a scoped thread per desk needs no shared state and
/// no unsafe code.
///
/// A member named by a channel that is not in `members` is skipped rather than
/// faked; the desk simply has one fewer seat, and the scheduler reports the
/// missing name if it is ever asked for.
pub(crate) fn group_by_desk<'a>(
    channels: &[Channel],
    members: impl Iterator<Item = &'a mut dyn SwarmMember>,
) -> Vec<Vec<&'a mut dyn SwarmMember>> {
    let mut loose: Vec<Option<&'a mut dyn SwarmMember>> = members.map(Some).collect();
    channels
        .iter()
        .map(|channel| {
            channel
                .members
                .iter()
                .filter_map(|wanted| {
                    let at = loose.iter().position(|held| {
                        held.as_ref().is_some_and(|member| member.id() == wanted)
                    })?;
                    loose[at].take()
                })
                .collect()
        })
        .collect()
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
