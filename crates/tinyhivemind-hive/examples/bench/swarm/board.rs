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
    HiveTurn, Sequence,
    dispatch::{DispatchConversation, DispatchKey},
    mention::{MentionAuthor, resolve as resolve_mentions},
    project_for,
    referral::{
        Referral, ReferralDecision, ReferralInput, ReferralOrigin, ReferralPolicy, referral,
    },
};

use super::{Channel, SwarmHost, SwarmMember, SwarmReport, format};

/// The scheduler's own state: the journals, what is in flight, and the tally.
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
pub(super) enum AskChannel {
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
    /// Where an ask is paid for, and how many one desk may make.
    asking: AskChannel,
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
        asking: AskChannel,
    ) -> Self {
        let count = channels.len();
        Self {
            host: SwarmHost::new(channels),
            channels,
            referrals,
            keep_trace,
            asking,
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
        members: &mut [&mut dyn SwarmMember],
        desk: usize,
        incoming: &Referral,
    ) -> Result<(), String> {
        let seat = seat_of(members, &incoming.target_id)?;
        let content = {
            let visible = self.host.journals[desk].clone();
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
    pub(super) fn take_turn(
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
        // Off the floor, an authorized turn is never spent asking: the ask
        // has its own channel and its own bound, and this turn is for
        // deliberating.
        let budget = self.asking.on_floor()
            && self.referrals.enabled
            && self.asks[desk] < self.ask_width();
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
        members: &mut [&mut dyn SwarmMember],
        desk: usize,
        agent_id: &str,
        content: &str,
        charge: bool,
    ) -> Sequence {
        let sequence = self.host.agent(desk, agent_id, content.to_owned());
        for member in &self.channels[desk].members {
            if let Ok(seat) = seat_of(members, member) {
                members[seat].absorb(content);
            }
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
        self.asking.width(self.channels.len().saturating_sub(1))
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
        members: &mut [&mut dyn SwarmMember],
        desk: usize,
    ) -> Result<bool, String> {
        if self.asking.on_floor() || !self.referrals.enabled || self.asks[desk] >= self.ask_width()
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

        let seat = seat_of(members, &asker)?;
        let Some(content) = members[seat].ask(&peers) else {
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
