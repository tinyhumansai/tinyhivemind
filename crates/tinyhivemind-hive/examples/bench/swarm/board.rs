//! The scheduler for one federation-wide run: the journals, the referrals in
//! flight, and the tally.
//!
//! [`Board`] is the piece of [`super::drive_swarm`] that turns "run one
//! episode per desk" into "run a federation of them": it owns the
//! [`SwarmHost`], the referrals each desk has queued for another channel, and
//! the running [`SwarmReport`]. Its methods are the whole surface the driving
//! loop touches — everything else is `#[cfg]`-free arithmetic and the real
//! `referral` fold doing the routing.

use std::collections::VecDeque;
use std::time::Duration;

use tinyhivemind_hive::{
    HiveTurn, Sequence, SessionMessage,
    dispatch::{DispatchConversation, DispatchKey},
    mention::{MentionAuthor, resolve as resolve_mentions},
    project_for,
    referral::{
        Referral, ReferralDecision, ReferralInput, ReferralOrigin, ReferralPolicy, referral,
    },
};

use super::{Channel, SwarmHost, SwarmMember, SwarmReport, format};

/// The scheduler's own state: the journals, what is in flight, and the tally.
/// One authorized turn, with everything its seat needs to fill it.
///
/// Carries its own copy of the projection rather than borrowing the journal,
/// so the desks can speak at the same time while the board stays untouched
/// until every one of them has finished.
pub(super) struct PlannedTurn {
    /// The desk this turn belongs to.
    pub(super) desk: usize,
    /// The seat, within that desk, the episode authorized.
    pub(super) seat: usize,
    /// Exactly what this turn may see.
    pub(super) visible: Vec<SessionMessage>,
    /// Peer channels this desk has not yet asked.
    pub(super) peers: Vec<String>,
    /// Whether this desk may spend the turn asking one of them.
    pub(super) budget: bool,
    /// The turn itself.
    pub(super) turn: HiveTurn,
}

/// What a seat actually said, once it had said it.
pub(super) struct SpokenTurn {
    /// The desk it was said on.
    pub(super) desk: usize,
    /// Who said it.
    pub(super) agent_id: String,
    /// What they said.
    pub(super) content: String,
    /// Whether the seat spent the turn asking another channel.
    pub(super) offered: bool,
}

/// Fill one planned turn, with no access to the board at all.
///
/// A free function rather than a method precisely because it must not touch
/// the board: this is the half that runs on every desk at once, and the only
/// state it may reach is the seats of its own desk.
///
/// # Errors
///
/// Returns a host-side failure, such as an agent process that did not answer.
pub(super) fn fill_turn(
    seats: &mut [&mut dyn SwarmMember],
    planned: &PlannedTurn,
) -> Result<SpokenTurn, String> {
    let peers: Vec<&str> = planned.peers.iter().map(String::as_str).collect();
    let ask = if planned.budget {
        seats[planned.seat].ask(&peers)
    } else {
        None
    };
    let (content, offered) = match ask {
        Some(body) => (body, true),
        None => (
            seats[planned.seat].speak(&planned.turn, &planned.visible)?,
            false,
        ),
    };
    Ok(SpokenTurn {
        desk: planned.desk,
        agent_id: planned.turn.agent_id.clone(),
        content,
        offered,
    })
}

/// Where a desk pays for a question it puts to another channel.
///
/// The two settings differ in one thing and it is the one that matters at
/// scale: whether asking costs the asking desk a turn it could have
/// deliberated with.
///
/// [`Self::OnFloor`] is what the recorded swarm numbers were taken against —
/// a member the episode authorized spends its authorized turn asking instead
/// of arguing. That is affordable in a federation of three and fatal in one of
/// fifty: the number of peers grows with the federation while the desk's turn
/// budget stays whatever its own size earns, so above roughly `budget / 3`
/// desks every desk spends its entire budget asking and never decides. The
/// measured collapse is total — 100% correct at twelve desks, 0.0% at
/// twenty-five, every desk `exhausted`.
///
/// [`Self::OffFloor`] writes the same question, to the same peer, read by the
/// same members, without the episode authorizing a turn for it. It is the
/// same move [ADR 0012] made for a private aside, for the same reason and with
/// the same accounting: what it spends is a model call, not a turn, and the
/// price is reported separately rather than hidden in `turns`.
///
/// The library is unchanged by either, and deliberately so.
/// `ReferralPolicy`'s own documentation says a width bound is *deliberately
/// absent* — "`max_hops` bounds the depth of a chain, and bounding its width
/// is the host's job, because only the host knows what a question costs it."
/// This type is that host doing that job.
///
/// [ADR 0012]: https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/adr/0012-an-exchange-round-spends-model-calls-not-turns.md
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AskChannel {
    /// A member spends the turn the episode authorized asking a peer channel,
    /// and may ask every peer once.
    OnFloor,
    /// A desk asks off the floor, taking no turn, at most `cap` times.
    ///
    /// `cap` is finite and stated by the caller rather than derived from the
    /// federation, which is the whole correction: a bound that grows with the
    /// number of peers is not a bound on what the federation costs.
    OffFloor {
        /// Questions one desk may put to other channels, across the episode.
        cap: usize,
    },
}

impl AskChannel {
    /// How many peer channels one desk may ask, given how many there are.
    ///
    /// On the floor this is every peer, which is the behaviour the recorded
    /// numbers were taken against and the defect described above. Off the
    /// floor it is the stated cap, whichever is smaller.
    fn width(self, peers: usize) -> usize {
        match self {
            Self::OnFloor => peers,
            Self::OffFloor { cap } => cap.min(peers),
        }
    }

    /// Whether a member may spend an authorized turn asking.
    const fn on_floor(self) -> bool {
        matches!(self, Self::OnFloor)
    }
}

/// How one desk reaches the rest of the federation.
///
/// Two mechanisms, and they answer different failures. [`AskChannel`] is a
/// *question to one peer*, and it fixes a desk that is wrong on its own. The
/// digest is a *reading published to every peer*, and it is the only thing
/// measured here that touches a federation whose desks are wrong about the
/// same thing.
///
/// The distinction is worth stating because the second looks like the first
/// done more times, and it is not. Asking every peer costs `D - 1` questions
/// and `D - 1` answers per desk; publishing costs **one** model call per desk,
/// and the row it writes reaches every peer's transcript. What it buys is
/// bounded by the same argument: at a hundred desks sharing seven blind spots,
/// the peer a desk would have asked shares its blind spot, and no number of
/// pairwise questions recovers an error the whole federation holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Exchange {
    /// Where a desk pays for a question it puts to one other channel.
    pub(crate) asking: AskChannel,
    /// Readings one desk publishes to every other channel, across the episode.
    ///
    /// Zero is off, and off is what every recorded number was taken against.
    pub(crate) digest: usize,
}

impl Exchange {
    /// Ask on the floor and publish nothing: the original arm.
    pub(crate) const ON_FLOOR: Self = Self {
        asking: AskChannel::OnFloor,
        digest: 0,
    };
}

pub(super) struct Board<'a> {
    /// The desks-and-journals host every episode runs against.
    host: SwarmHost,
    /// The channels this run was started with, in federation order.
    channels: &'a [Channel],
    /// Whether a member may spend a turn asking another channel, and how deep
    /// a chain of such asks may run.
    referrals: ReferralPolicy,
    /// Whether to record a human-readable line per message for the trace view.
    keep_trace: bool,
    /// Referrals routed to each desk and not yet run.
    pending: Vec<VecDeque<Referral>>,
    /// Peer channels each desk has already asked.
    asks: Vec<usize>,
    /// Where an ask is paid for, how many one desk may make, and how many
    /// readings it publishes to the whole federation.
    exchange: Exchange,
    /// Readings each desk has already published federation-wide.
    published: Vec<usize>,
    /// Members of each desk offered the digest so far, so publishing rotates
    /// rather than always landing on seat zero.
    publishers: Vec<usize>,
    /// Members of each desk offered the off-floor ask so far, so the offer
    /// rotates rather than always landing on seat zero.
    askers: Vec<usize>,
    /// What this run has decided and spent so far.
    report: SwarmReport,
}

impl<'a> Board<'a> {
    /// Set up a fresh scheduler for one federation-wide run.
    pub(super) fn new(
        channels: &'a [Channel],
        referrals: ReferralPolicy,
        keep_trace: bool,
        exchange: Exchange,
    ) -> Self {
        let count = channels.len();
        Self {
            host: SwarmHost::new(channels),
            channels,
            referrals,
            keep_trace,
            exchange,
            published: vec![0; count],
            publishers: vec![0; count],
            askers: vec![0; count],
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
        }
    }

    /// A read-only view of the desks-and-journals host.
    pub(super) fn host(&self) -> &SwarmHost {
        &self.host
    }

    /// A mutable view of the desks-and-journals host.
    pub(super) fn host_mut(&mut self) -> &mut SwarmHost {
        &mut self.host
    }

    /// Whether a desk has nothing waiting to be answered.
    pub(super) fn pending_empty(&self, desk: usize) -> bool {
        self.pending[desk].is_empty()
    }

    /// Take the next referral queued for a desk, if one is waiting.
    pub(super) fn pop_pending(&mut self, desk: usize) -> Option<Referral> {
        self.pending[desk].pop_front()
    }

    /// Charge time spent inside the library to this run's report.
    pub(super) fn add_library_time(&mut self, elapsed: Duration) {
        self.report.library_time += elapsed;
        self.report.step_calls = self.report.step_calls.saturating_add(1);
    }

    /// Consume the board and hand back the report it has been keeping.
    pub(super) fn into_report(self) -> SwarmReport {
        self.report
    }

    /// Give up on everything routed to a desk that has already finished.
    ///
    /// An answer that arrives after the desk that asked has closed is
    /// information the federation paid for and cannot use. It is counted rather
    /// than quietly dropped.
    pub(super) fn strand(&mut self, desk: usize) {
        self.report.stranded = self
            .report
            .stranded
            .saturating_add(u32::try_from(self.pending[desk].len()).unwrap_or(0));
        self.pending[desk].clear();
    }

    /// Run one turn caused by a message that arrived from another channel.
    pub(super) fn deliver(
        &mut self,
        members: &mut [Vec<&mut dyn SwarmMember>],
        desk: usize,
        incoming: &Referral,
    ) -> Result<(), String> {
        let seat = seat_of(&members[desk], &incoming.target_id)?;
        let content = {
            let visible = self.host.journals[desk].clone();
            members[desk][seat].answer(incoming, &visible)?
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

    /// What a desk needs to know before it can fill an authorized turn.
    ///
    /// Split out of running the turn because the two happen at different
    /// times: planning reads the board and is microseconds, while filling the
    /// turn is a model call and is seconds. Every desk plans first, every desk
    /// then speaks at once, and the answers land in desk order — see
    /// [`crate::parallel::map_mut_in_order`] for why that last part is not
    /// optional.
    ///
    /// # Errors
    ///
    /// Returns a host-side failure when the authorized speaker is not seated
    /// on the desk the turn belongs to.
    pub(super) fn plan_turn(
        &self,
        members: &[Vec<&mut dyn SwarmMember>],
        desk: usize,
        turn: &HiveTurn,
    ) -> Result<PlannedTurn, String> {
        let seat = seat_of(&members[desk], &turn.agent_id)?;
        let peers: Vec<String> = self
            .channels
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != desk)
            .map(|(_, channel)| channel.id.clone())
            .skip(self.asks[desk])
            .collect();
        // Off the floor, an authorized turn is never spent asking: the ask
        // has its own channel and its own bound, and this turn is for
        // deliberating.
        let budget =
            self.exchange.asking.on_floor() && self.referrals.enabled && self.asks[desk] < self.ask_width();
        Ok(PlannedTurn {
            desk,
            seat,
            visible: project_for(turn, &self.host.journals[desk]),
            peers,
            budget,
            turn: turn.clone(),
        })
    }

    /// Append what a desk said, offer it to the desk, and route any question.
    ///
    /// The sequential half of a turn, run in desk order after every desk has
    /// spoken.
    ///
    /// # Errors
    ///
    /// Returns the library's own error text when routing fails.
    pub(super) fn land_turn(
        &mut self,
        members: &mut [Vec<&mut dyn SwarmMember>],
        spoken: &SpokenTurn,
        budget: bool,
    ) -> Result<(), String> {
        let desk = spoken.desk;
        let sequence = self.commit(members, desk, &spoken.agent_id, &spoken.content);
        let routed =
            budget && self.route(desk, &spoken.agent_id, &spoken.content, sequence, 0, None)?;
        // A turn spent asking is spent whether or not the question found its
        // way out, so the budget is charged either way.
        if spoken.offered || routed {
            self.asks[desk] = self.asks[desk].saturating_add(1);
        }
        Ok(())
    }

    /// Append one reply, offer it to everybody on the desk, and count the turn.
    fn commit(
        &mut self,
        members: &mut [Vec<&mut dyn SwarmMember>],
        desk: usize,
        agent_id: &str,
        content: &str,
    ) -> Sequence {
        self.commit_charging(members, desk, agent_id, content, true)
    }

    /// The same, saying whether the row is charged to the floor.
    ///
    /// An off-floor ask is an agent invocation and is *not* a turn, so it is
    /// priced in its own column rather than folded into `turns`. Charging it
    /// as a turn would make the off-floor arm look as if it had spent the
    /// budget it exists to preserve, and the comparison against the on-floor
    /// arm — which is a comparison of exactly that — would say nothing.
    fn commit_charging(
        &mut self,
        members: &mut [Vec<&mut dyn SwarmMember>],
        desk: usize,
        agent_id: &str,
        content: &str,
        charge: bool,
    ) -> Sequence {
        let sequence = self.host.agent(desk, agent_id, content.to_owned());
        // Every member of the desk is offered the line, which is what makes
        // one member's question worth a turn to the whole desk. Members are
        // grouped by desk, so this is a walk over this desk's seats rather
        // than a lookup per seat across the whole federation.
        for member in &mut members[desk] {
            member.absorb(content);
        }
        if charge {
            self.report.turns = self.report.turns.saturating_add(1);
        }
        if content.trim_start().starts_with("!defer") {
            self.report.defers = self.report.defers.saturating_add(1);
        }
        if self.keep_trace {
            self.report.trace.push(format::line(
                self.channels,
                desk,
                sequence,
                agent_id,
                content,
            ));
        }
        sequence
    }

    /// Peer channels one desk may ask, under this run's ask channel.
    fn ask_width(&self) -> usize {
        self.exchange.asking.width(self.channels.len().saturating_sub(1))
    }

    /// Put one question to another channel **without taking a turn for it**.
    ///
    /// Returns whether a question actually went out. Called between turns
    /// rather than during one, so the episode's turn budget is untouched and
    /// the desk keeps every turn its own size earned for deliberating.
    ///
    /// The row it writes is a plain desk row carrying no trace marker, so the
    /// episode's own fold reads nothing out of it — the same safety argument
    /// off-floor exchange rests on, and with the same stated caveat: a row
    /// consumes a sequence number, and the quorum window and salience decay
    /// read raw sequence distance. At the windows this harness runs (100
    /// against episodes of tens of turns) that is far from binding, and the
    /// siloed control alongside is what would show it if it ever were.
    ///
    /// The offer rotates through the desk's members rather than always going
    /// to the same seat, so one member is not made the desk's switchboard.
    pub(super) fn ask_off_floor(
        &mut self,
        members: &mut [Vec<&mut dyn SwarmMember>],
        desk: usize,
    ) -> Result<bool, String> {
        if self.exchange.asking.on_floor() || !self.referrals.enabled || self.asks[desk] >= self.ask_width()
        {
            return Ok(false);
        }
        let peers: Vec<&str> = self
            .channels
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != desk)
            .map(|(_, channel)| channel.id.as_str())
            .skip(self.asks[desk])
            .collect();
        if peers.is_empty() {
            return Ok(false);
        }
        let seats = &self.channels[desk].members;
        if seats.is_empty() {
            return Ok(false);
        }
        let asker = seats[self.askers[desk] % seats.len()].clone();
        self.askers[desk] = self.askers[desk].saturating_add(1);

        let seat = seat_of(&members[desk], &asker)?;
        let Some(content) = members[desk][seat].ask(&peers) else {
            return Ok(false);
        };
        let sequence = self.commit_charging(members, desk, &asker, &content, false);
        // Counted whether or not the question found a route, exactly as the
        // on-floor path charges a turn spent asking either way. A question
        // that resolved to nobody still cost a model call.
        self.asks[desk] = self.asks[desk].saturating_add(1);
        self.report.off_floor_asks = self.report.off_floor_asks.saturating_add(1);
        self.route(desk, &asker, &content, sequence, 0, None)?;
        Ok(true)
    }

    /// Publish one desk's reading of the whole slate to every other channel.
    ///
    /// Returns whether one went out. Like an off-floor ask this takes no turn
    /// and is priced in its own column, and unlike one it is not a question:
    /// nobody answers it, so it costs exactly one model call however many
    /// desks are in the federation.
    ///
    /// **The row lands on every peer desk authored by a member of the
    /// publishing desk**, which is what makes it safe. `episode::step` folds a
    /// trace only when its author is a current member of the desk it is
    /// folding, so a digest deposits information into every reader's view and
    /// support into nobody's standings. It is the same guarantee a referral's
    /// `!evidence` carries, arrived at from the other direction: there the
    /// answer is restated by a member of the receiving desk and counts as that
    /// desk's own evidence, here it stays foreign and counts as nothing until
    /// a member of the desk spends a turn saying it.
    ///
    /// It does consume a sequence number on every peer's journal, and under
    /// [`Basis::Sequence`] that shortens their quorum window by one per
    /// digest. That is exactly the interaction [`Basis::Live`] exists for, and
    /// the arm is run both ways for that reason.
    ///
    /// [`Basis::Sequence`]: tinyhivemind_hive::Basis::Sequence
    /// [`Basis::Live`]: tinyhivemind_hive::Basis::Live
    pub(super) fn publish_digest(
        &mut self,
        members: &mut [Vec<&mut dyn SwarmMember>],
        desk: usize,
    ) -> Result<bool, String> {
        if self.published[desk] >= self.exchange.digest {
            return Ok(false);
        }
        let seats = &self.channels[desk].members;
        if seats.is_empty() {
            return Ok(false);
        }
        let author = seats[self.publishers[desk] % seats.len()].clone();
        self.publishers[desk] = self.publishers[desk].saturating_add(1);
        let seat = seat_of(&members[desk], &author)?;
        let Some(content) = members[desk][seat].publish() else {
            return Ok(false);
        };
        // Counted before the appends, so a publisher that reaches nobody --
        // a federation of one -- still records the call it made.
        self.published[desk] = self.published[desk].saturating_add(1);
        self.report.digests = self.report.digests.saturating_add(1);
        for peer in 0..self.channels.len() {
            if peer == desk {
                continue;
            }
            self.commit_charging(members, peer, &author, &content, false);
            self.report.crossings = self.report.crossings.saturating_add(1);
        }
        Ok(true)
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
