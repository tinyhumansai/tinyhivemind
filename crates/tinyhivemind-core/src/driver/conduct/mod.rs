//! One completion-driven episode as a host steps it: the desk, its
//! conversations, and the rules between them.
//!
//! [`CompletionDriver`] folds one episode and never runs a turn. A live
//! episode is more than one fold: the desk's, and a child episode for every
//! conversation an `ask` opens (ADR 0023). Between them sit rules the driver
//! cannot hold because they span both, or because they are about what a
//! turn did *not* do:
//!
//! - **Conversations.** An ask roots one at its row, with every seat it
//!   named as a participant -- `ask` names one, `ask_teammates` names a
//!   group, and a group is one conversation, not one each (ADR 0026). It runs ahead of desk turns, because it is what unblocks
//!   one. It concludes when every seat asked has completed, or at a wall, or
//!   when nothing is due anywhere; its outcome is cross-posted to the asker
//!   as a private row per seat, which releases the asker's hold.
//! - **Nudges** (ADR 0024). A desk seat that holds open work, ran for it and
//!   has been shown everything is told once, per assignment, and owed a turn.
//!   A seat asked that took its turn and did not answer is told once and
//!   owed a turn; a second silence stands.
//! - **Sorting.** What a wave said lands in the channel it belongs to. A
//!   broadcast or an ask made inside a conversation is desk work; anything
//!   but a post or a completion inside one is dropped.
//! - **Refusals.** A completion the ledger refuses is explained to the seat
//!   on the desk; a spent broadcast budget completes the seat with the work.
//! - **Walls.** Turns per conversation, and turns per episode.
//!
//! [`Conductor`] holds all of that as state and folds. It appends nothing:
//! every row is the host's, so it hands the host [`Step`]s -- a [`Note`] to
//! append, a [`Commit`] to append and report the sequence of, an [`Event`]
//! to log -- and takes the sequence back. One wave, from the host's side:
//!
//! 1. [`begin_wave`](Conductor::begin_wave): the nudges due, as steps.
//! 2. [`turns`](Conductor::turns): who runs, where, and from which row.
//! 3. For each turn, [`open_turn`](Conductor::open_turn) with the rows the
//!    host will show it: the [`EpisodeBrief`] for the prompt.
//! 4. Run the turns; [`record`](Conductor::record) what each called.
//! 5. [`step`](Conductor::step) until it returns nothing, appending each
//!    note, appending each commit and reporting its sequence through
//!    [`committed`](Conductor::committed), and logging each event.
//!
//! [`finished`](Conductor::finished) says when to stop.

mod child;
mod steps;
#[cfg(test)]
mod test;
mod types;
mod wave;

use std::collections::{BTreeMap, BTreeSet};

use crate::hive::CompletionEpisodeState;
use crate::runtime::speech::{ToolCall, Utterance};
use crate::runtime::{Conversation, Sequence};

use crate::driver::engine::{BroadcastRouting, Channel, ConversationView, EpisodeBrief};
use crate::driver::{BoundAgent, CompletionDriver, DriverState, Error, Result};
use child::{Child, Concluded};
pub use steps::{Commit, Event, Note, Refusal, Step, Turn};
pub use types::{ConductPolicy, ConductorState, Door, starters};
use wave::Wave;

/// The desk episode, its conversations, and the rules between them.
pub struct Conductor<'a, A: BoundAgent> {
    driver: &'a CompletionDriver<'a, A>,
    routing: BroadcastRouting<'a>,
    chat: String,
    desk_name: String,
    policy: ConductPolicy,
    state: DriverState,
    children: BTreeMap<Sequence, Child>,
    concluded: Vec<Concluded>,
    /// How many concluded conversations each seat has been shown.
    shown: BTreeMap<String, usize>,
    /// The assignment each seat was last nudged for on the desk.
    desk_nudged: BTreeMap<String, Sequence>,
    /// Seats held on the host, by the thread they parked in (`None` for the
    /// desk): not nudged, not stalled, not proposed, until released.
    parked: BTreeMap<String, Option<Sequence>>,
    turns: u64,
    waves: u64,
    discharged: u64,
    wave: Wave,
}

impl<A: BoundAgent> std::fmt::Debug for Conductor<'_, A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Conductor")
            .field("chat", &self.chat)
            .field("turns", &self.turns)
            .field("waves", &self.waves)
            .field("conversations", &self.children.keys().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

impl<'a, A: BoundAgent> Conductor<'a, A> {
    /// Open the desk episode. Every member is seated so a handoff can reach
    /// any seat; the ones the door route passed over are completed at once
    /// at `opened_at`, the task's row -- idle, and reopened by any broadcast
    /// that finds them.
    ///
    /// # Errors
    ///
    /// [`Error::UnknownStarter`] for a starter that is not a member, and
    /// [`Error::NoStarters`] for none at all; otherwise the episode refusing
    /// its members, or the driver its start.
    pub fn open(
        driver: &'a CompletionDriver<'a, A>,
        routing: BroadcastRouting<'a>,
        policy: ConductPolicy,
        door: Door,
    ) -> Result<Self> {
        if door.starters.is_empty() {
            return Err(Error::NoStarters);
        }
        if let Some(seat) = door
            .starters
            .iter()
            .find(|seat| !door.members.contains(seat))
        {
            return Err(Error::UnknownStarter { seat: seat.clone() });
        }
        // Everyone is assigned at the task's row, and the passed-over seats
        // are completed on that same row: set directly, because a completion
        // applied as an event must land strictly above its assignment, and
        // the task's row may be the log's first, at sequence zero.
        let mut episode = CompletionEpisodeState::opened(
            Conversation {
                desk_id: door.chat.clone(),
                desk_name: door.desk_name.clone(),
                thread_root: None,
            },
            door.opened_at,
            door.members.iter().map(String::as_str),
        )?;
        for participant in &mut episode.participants {
            if !door.starters.contains(&participant.agent_id) {
                for record in &mut participant.assignments {
                    record.completed_at = Some(door.opened_at);
                }
            }
        }
        let state = driver.start(episode)?;
        Ok(Self {
            driver,
            routing,
            chat: door.chat,
            desk_name: door.desk_name,
            policy,
            state,
            children: BTreeMap::new(),
            concluded: Vec::new(),
            shown: BTreeMap::new(),
            desk_nudged: BTreeMap::new(),
            parked: BTreeMap::new(),
            turns: 0,
            waves: 0,
            discharged: 0,
            wave: Wave::default(),
        })
    }

    /// Over: the desk is quiescent and no conversation is open.
    #[must_use]
    pub fn finished(&self) -> bool {
        // A parked seat is not a finished one, even where its work closed
        // some other way -- a broadcast it made in the same turn spending
        // the budget, say. Ending the episode there would strand whatever
        // the host queued to hold it: the operator answers an approval that
        // no longer has a loop to return to.
        self.state.quiescent() && self.children.is_empty() && self.parked.is_empty()
    }

    /// The desk episode's state.
    #[must_use]
    pub const fn state(&self) -> &DriverState {
        &self.state
    }

    /// The chat every tool call names.
    #[must_use]
    pub fn chat(&self) -> &str {
        &self.chat
    }

    /// Seat turns run so far.
    #[must_use]
    pub const fn turns_run(&self) -> u64 {
        self.turns
    }

    /// Waves proposed so far.
    #[must_use]
    pub const fn waves(&self) -> u64 {
        self.waves
    }

    /// Seats completed with their work for a spent broadcast budget.
    #[must_use]
    pub const fn discharged(&self) -> u64 {
        self.discharged
    }

    /// Conversations concluded so far.
    #[must_use]
    pub fn conversations(&self) -> usize {
        self.concluded.len()
    }

    /// Start a wave: the desk seats nothing will wake are told once, per
    /// assignment, and owed a turn.
    pub fn begin_wave(&mut self) -> Vec<Step> {
        self.waves += 1;
        let mut steps = Vec::new();
        for seat in self.state.stalled() {
            // A parked seat is waiting on the host, not on the desk.
            if self.parked.contains_key(&seat) {
                continue;
            }
            let assigned_at = self
                .state
                .episode()
                .participants
                .iter()
                .find(|participant| participant.agent_id == seat)
                .and_then(|participant| participant.open())
                .map(|record| record.assigned_at);
            if self.desk_nudged.get(&seat) == assigned_at.as_ref() {
                continue;
            }
            if let Some(at) = assigned_at {
                self.desk_nudged.insert(seat.clone(), at);
            }
            steps.push(Step::Event(Event::Nudged {
                seat: seat.clone(),
                thread: None,
            }));
            steps.push(Step::Note(Note {
                body: "you still have open work and nothing new has come in. Finish your part \
                       with what you have, or hand the part that belongs to a teammate over to \
                       them, or ask one of them a question. Those three are the only \
                       things that record anything here; reading, searching and every other \
                       kind of work says nothing on its own, so a turn spent only on those \
                       leaves this desk as it was."
                    .to_owned(),
                thread: None,
                only_for: Some(seat.clone()),
            }));
            self.state.owe_turn(&seat);
        }
        for child in self.children.values_mut() {
            child.turned.clear();
        }
        steps
    }

    /// The turns due this wave: one per seat, conversations first, because a
    /// conversation is what unblocks a desk turn.
    ///
    /// An empty wave with conversations open concludes them all, without an
    /// answer, in the steps that follow.
    ///
    /// # Errors
    ///
    /// [`Error::Stalled`]: nothing is due anywhere, no conversation is open,
    /// and the desk holds open work.
    pub fn turns(&mut self) -> Result<Vec<Turn>> {
        let mut taken: BTreeSet<String> = BTreeSet::new();
        let mut turns = Vec::new();
        for child in self.children.values() {
            for seat in self.pending(&child.state)? {
                if self.parked.contains_key(&seat) || !taken.insert(seat.clone()) {
                    continue;
                }
                // The ask row is the first thing a seat is shown in the
                // conversation it roots: a first turn there starts just
                // below it, which for a root at zero is nowhere.
                let since = child
                    .state
                    .seen()
                    .delivered_through
                    .get(&seat)
                    .copied()
                    .or_else(|| child.root.0.checked_sub(1).map(Sequence));
                turns.push(Turn {
                    channel: Channel::Thread {
                        root: child.root,
                        others: child.others(&seat),
                        opened_it: seat == child.asker,
                    },
                    seat,
                    since,
                });
            }
        }
        for seat in self.pending(&self.state)? {
            if self.parked.contains_key(&seat) || !taken.insert(seat.clone()) {
                continue;
            }
            let since = self.state.seen().delivered_through.get(&seat).copied();
            turns.push(Turn {
                seat,
                channel: Channel::Desk,
                since,
            });
        }
        if turns.is_empty() && self.children.is_empty() && self.parked.is_empty() {
            return Err(Error::Stalled {
                seats: self.state.stalled(),
            });
        }
        // Nothing due with a seat parked is a wait, not an end: no
        // conversation concludes for it.
        self.wave.begin(turns.is_empty() && self.parked.is_empty());
        Ok(turns)
    }

    /// Everything needed to rebuild this conductor, or `None` while the
    /// host holds a commit it has not reported.
    ///
    /// A host checkpoints after every committed row. The wave in progress
    /// travels with the snapshot, so a crash replays at most that one row:
    /// the conductor comes back mid-wave with the same seats having spoken
    /// and the same steps still to take.
    ///
    /// `None` means the host is holding a commit whose sequence it has not
    /// reported through [`committed`](Self::committed). The conductor cannot
    /// say whether that row reached the journal, so it will not write down a
    /// claim either way; the caller reports the sequence and asks again.
    #[must_use]
    pub fn snapshot(&self) -> Option<ConductorState> {
        if !self.wave.recordable() {
            return None;
        }
        Some(ConductorState {
            chat: self.chat.clone(),
            desk_name: self.desk_name.clone(),
            state: self.state.clone(),
            children: self
                .children
                .iter()
                .map(|(root, child)| (*root, child.clone()))
                .collect(),
            concluded: self.concluded.clone(),
            shown: self.shown.clone(),
            desk_nudged: self.desk_nudged.clone(),
            parked: self.parked.clone(),
            turns: self.turns,
            waves: self.waves,
            discharged: self.discharged,
            wave: self.wave.clone(),
        })
    }

    /// Rebuild a conductor from a snapshot, on a driver and a routing the
    /// host supplies again.
    ///
    /// The episode carries on where it paused: the same conversations are
    /// open, the same seats are held, a seat already nudged for its
    /// assignment is not nudged again for it, and a wave that was in
    /// progress resumes mid-wave. A caller drains
    /// [`step`](Self::step) before proposing a new wave, or the restored
    /// steps are dropped.
    ///
    /// # Errors
    ///
    /// The driver refusing the episode or any of its conversations -- a
    /// state naming a seat or a desk this hive does not have -- and
    /// [`Error::InconsistentSnapshot`] for a snapshot that disagrees with
    /// itself: a conversation filed under the wrong root, a cursor past the
    /// conversations it points into, or a seat held that this desk does not
    /// seat.
    pub fn resume(
        driver: &'a CompletionDriver<'a, A>,
        routing: BroadcastRouting<'a>,
        policy: ConductPolicy,
        snapshot: ConductorState,
    ) -> Result<Self> {
        let state = driver.resume(snapshot.state)?;
        // A snapshot is the host's file, not the conductor's memory: every
        // part of it is validated here rather than trusted, because the
        // alternative is an episode that resumes and then misbehaves waves
        // later with nothing left to say why.
        let mut children = BTreeMap::new();
        for (root, mut child) in snapshot.children {
            if root != child.root {
                return Err(Error::InconsistentSnapshot {
                    reason: format!(
                        "a conversation filed under {} calls itself {}",
                        root.0, child.root.0
                    ),
                });
            }
            // The same validation the desk episode gets: the conversation
            // names this desk, and its seats are seats of this hive.
            child.state = driver.resume(child.state)?;
            children.insert(root, child);
        }
        // A cursor into the concluded list, for a seat that has been shown
        // some of them. Past the end it would panic the first time that
        // seat is briefed.
        for (seat, cursor) in &snapshot.shown {
            if *cursor > snapshot.concluded.len() {
                return Err(Error::InconsistentSnapshot {
                    reason: format!(
                        "@{seat} has been shown {cursor} conversations of {}",
                        snapshot.concluded.len()
                    ),
                });
            }
        }
        // A seat held on the host that this desk does not seat would be held
        // for ever: it is never proposed, and nothing can release it into a
        // wave.
        for seat in snapshot.parked.keys() {
            if !state
                .episode()
                .participants
                .iter()
                .any(|participant| &participant.agent_id == seat)
            {
                return Err(Error::InconsistentSnapshot {
                    reason: format!("@{seat} is held but is not seated at this desk"),
                });
            }
        }
        Ok(Self {
            driver,
            routing,
            chat: snapshot.chat,
            desk_name: snapshot.desk_name,
            policy,
            state,
            children,
            concluded: snapshot.concluded,
            shown: snapshot.shown,
            desk_nudged: snapshot.desk_nudged,
            parked: snapshot.parked,
            turns: snapshot.turns,
            waves: snapshot.waves,
            discharged: snapshot.discharged,
            wave: snapshot.wave,
        })
    }

    /// The seats held on the host, in seat order. An empty wave with any of
    /// these is the host's to end: it releases one with
    /// [`resume_seat`](Self::resume_seat), or gives up.
    #[must_use]
    pub fn parked(&self) -> Vec<String> {
        self.parked.keys().cloned().collect()
    }

    /// A turn stopped on something only the host can settle -- an approval,
    /// typically. What it called before it stopped is recorded as any turn's
    /// calls are; the seat is then held where it parked: not nudged for
    /// silence, not counted toward a stall, and not proposed again until the
    /// host releases it. A parked askee's conversation waits with it.
    pub fn record_parked(&mut self, turn: &Turn, calls: impl IntoIterator<Item = ToolCall>) {
        self.record(turn, calls);
        let thread = turn.thread();
        if let Some(child) = thread.and_then(|root| self.children.get_mut(&root)) {
            // Not a silence: the askee is coming back to this conversation,
            // so it is not nudged for having said nothing in it.
            child.turned.remove(&turn.seat);
        }
        self.parked.insert(turn.seat.clone(), thread);
        self.wave.event(Event::Parked {
            seat: turn.seat.clone(),
            thread,
        });
    }

    /// The host settled what a seat parked on: it is owed a turn where it
    /// parked, in the next wave. A seat that is not parked is left as it is.
    pub fn resume_seat(&mut self, seat: &str) {
        let Some(thread) = self.parked.remove(seat) else {
            return;
        };
        match thread.and_then(|root| self.children.get_mut(&root)) {
            Some(child) => child.state.owe_turn(seat),
            None => self.state.owe_turn(seat),
        }
        self.wave.event(Event::Resumed {
            seat: seat.to_owned(),
            thread,
        });
    }

    fn pending(&self, state: &DriverState) -> Result<Vec<String>> {
        Ok(self
            .driver
            .pending_round(state)?
            .agents()
            .iter()
            .map(|pending| pending.hive_agent_id.to_owned())
            .collect())
    }

    /// Open a turn: record that the seat is shown everything through
    /// `latest` -- the log's newest row, or `None` for a log with none --
    /// and that it ran for what it holds, and build its brief.
    /// `new_rows` are the rows above [`Turn::since`] in the turn's channel,
    /// rendered by the host; `transcript` is any thread of the desk, whole,
    /// for the conversations the seat is or was in.
    pub fn open_turn(
        &mut self,
        turn: &Turn,
        latest: Option<Sequence>,
        new_rows: Vec<String>,
        mut transcript: impl FnMut(Sequence) -> Vec<String>,
    ) -> EpisodeBrief {
        match turn.channel {
            Channel::Thread { root, .. } => {
                // Read before the child is borrowed mutably below, and before
                // this turn's own mutations: what the seat is owed is what it
                // has already been part of, which this turn has not changed.
                let mine = self.views_unconsumed(&turn.seat, &mut transcript);
                if let Some(child) = self.children.get_mut(&root) {
                    if let Some(latest) = latest {
                        child.state.delivered(&turn.seat, latest);
                    }
                    child.state.turn_started(&turn.seat);
                    child.turns += 1;
                    child.turned.insert(turn.seat.clone());
                    return EpisodeBrief::for_turn(
                        &child.state,
                        self.chat.clone(),
                        turn.seat.clone(),
                        turn.channel.clone(),
                        new_rows,
                        mine.clone(),
                    );
                }
                EpisodeBrief::for_turn(
                    &self.state,
                    self.chat.clone(),
                    turn.seat.clone(),
                    turn.channel.clone(),
                    new_rows,
                    mine,
                )
            }
            Channel::Desk => {
                if let Some(latest) = latest {
                    self.state.delivered(&turn.seat, latest);
                }
                self.state.turn_started(&turn.seat);
                let views = self.views(&turn.seat, &mut transcript);
                EpisodeBrief::for_turn(
                    &self.state,
                    self.chat.clone(),
                    turn.seat.clone(),
                    Channel::Desk,
                    new_rows,
                    views,
                )
            }
        }
    }

    /// Every conversation this seat was a party to, cursor ignored: exactly
    /// the roots a thread turn's transcript callback will be asked for.
    ///
    /// (Unlinked on purpose: the read that asks for them, `views_unconsumed`,
    /// is private, and `cargo doc` runs with `-D warnings`.)
    ///
    /// # Why a host needs this
    ///
    /// Because that callback is a prefetch, not a lookup. A host reads the rows
    /// it expects to be asked for and hands back what it has; a root it did not
    /// read yields an empty transcript, and a `ConversationView` with no
    /// transcript renders as a heading promising an exchange and delivering
    /// none. So a host opening a **thread** turn must prefetch these, and
    /// [`Self::shown_conversations`] is the wrong list for that — it is the
    /// cursored one a desk turn is owed, and it is deliberately empty for a
    /// seat whose digest has already been spent.
    #[must_use]
    pub fn conversations_involving(&self, seat: &str) -> Vec<Sequence> {
        self.concluded
            .iter()
            .filter(|done| done.involves(seat))
            .map(|done| done.root)
            .chain(
                self.children
                    .values()
                    .filter(|child| child.involves(seat))
                    .map(|child| child.root),
            )
            .collect()
    }

    /// The conversations `seat` would be shown on its next desk turn, by
    /// root: those concluded since it last spoke, and any still in progress.
    /// A host that reads its log asynchronously fetches these transcripts
    /// before [`open_turn`](Self::open_turn), which reads them by root.
    #[must_use]
    pub fn shown_conversations(&self, seat: &str) -> Vec<Sequence> {
        let cursor = self.shown.get(seat).copied().unwrap_or(0);
        self.concluded[cursor..]
            .iter()
            .filter(|done| done.involves(seat))
            .map(|done| done.root)
            .chain(
                self.children
                    .values()
                    .filter(|child| child.involves(seat))
                    .map(|child| child.root),
            )
            .collect()
    }

    /// The conversations a seat is shown on a desk turn: those concluded
    /// since it last spoke, whole, once; and any still in progress.
    /// The conversations a seat is shown on a **thread** turn: every one it
    /// was a party to in this episode, read without consuming anything.
    ///
    /// # Why this does not call [`Self::views`]
    ///
    /// Because that one is "since you last spoke" and says so by advancing
    /// `shown`. A thread turn borrowing it would consume the seat's digest:
    /// the conversation would be shown inside the thread and then be missing
    /// from the desk turn that the cursor exists to serve. Two different
    /// questions -- "what is new for you on the desk" and "what have you
    /// already been part of" -- so two reads, and only the first one counts.
    ///
    /// Whole history rather than a window, for the same reason: a seat
    /// answering a second question needs what it said the first time, however
    /// long ago that was. A live run asked one teammate twice on two threads
    /// and the second brief carried no trace of the first, so it answered
    /// blind and the asker paid to restate context the askee itself had
    /// produced.
    fn views_unconsumed(
        &self,
        seat: &str,
        transcript: &mut impl FnMut(Sequence) -> Vec<String>,
    ) -> Vec<ConversationView> {
        let mut views: Vec<ConversationView> = self
            .concluded
            .iter()
            .filter(|done| done.involves(seat))
            .map(|done| done.view(seat, transcript(done.root)))
            .collect();
        views.extend(
            self.children
                .values()
                .filter(|child| child.involves(seat))
                .map(|child| child.view(seat, transcript(child.root))),
        );
        views
    }

    fn views(
        &mut self,
        seat: &str,
        transcript: &mut impl FnMut(Sequence) -> Vec<String>,
    ) -> Vec<ConversationView> {
        let cursor = self.shown.entry(seat.to_owned()).or_insert(0);
        let mut views: Vec<ConversationView> = self.concluded[*cursor..]
            .iter()
            .filter(|done| done.involves(seat))
            .map(|done| done.view(seat, transcript(done.root)))
            .collect();
        *cursor = self.concluded.len();
        views.extend(
            self.children
                .values()
                .filter(|child| child.involves(seat))
                .map(|child| child.view(seat, transcript(child.root))),
        );
        views
    }

    /// What a turn called, in call order, once the host has closed it.
    /// Sorted into the channel each belongs to: a broadcast or an ask made
    /// inside a conversation is desk work.
    pub fn record(&mut self, turn: &Turn, calls: impl IntoIterator<Item = ToolCall>) {
        self.turns += 1;
        for call in calls {
            let ToolCall::Speak(utterance) = call else {
                continue;
            };
            match (turn.thread(), &utterance) {
                (None, _) => self.wave.desk.push((turn.seat.clone(), utterance, None)),
                (Some(root), Utterance::Broadcast { .. } | Utterance::Ask { .. }) => {
                    self.wave
                        .desk
                        .push((turn.seat.clone(), utterance, Some(root)));
                }
                (Some(root), _) => self.wave.thread.push((root, turn.seat.clone(), utterance)),
            }
        }
    }
}
