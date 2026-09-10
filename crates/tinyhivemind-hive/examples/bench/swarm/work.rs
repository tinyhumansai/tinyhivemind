//! One unit of work a seat fills, and the filling of it.
//!
//! A desk owes the federation one of two things on any pass: the round its own
//! episode authorized, or an answer to a question another channel put to it.
//! Both are planned against the board, filled **with no access to the board at
//! all**, and landed back onto it — and that three-part split is what lets
//! every desk's model call run at the same time while the journals stay
//! untouched until every one of them has returned.
//!
//! The types and the two `fill_*` functions live here rather than in
//! [`super::board`] for exactly that reason: nothing in this file may reach
//! the board, and keeping it in its own module is how that stays true.

use tinyhivemind_hive::{HiveTurn, SessionMessage, referral::Referral};

use super::SwarmMember;

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

/// One referral answer, with everything its seat needs to fill it.
///
/// The same plan/fill/land split a turn gets, and for the same reason:
/// answering a referral is a model call of several seconds on a live desk, and
/// leaving it in the scheduler's sequential phase would serialise every
/// pending answer in the federation however wide `--jobs` was set.
pub(super) struct PlannedAnswer {
    /// The desk that owes the answer.
    pub(super) desk: usize,
    /// The seat, within that desk, the referral named.
    pub(super) seat: usize,
    /// The question that arrived.
    pub(super) incoming: Referral,
    /// The journal as it stood when the answer was planned.
    pub(super) visible: Vec<SessionMessage>,
}

/// What a seat answered, once it had answered it.
pub(super) struct SpokenAnswer {
    /// The desk it was answered on.
    pub(super) desk: usize,
    /// The question it answers, carrying the hop and origin the reply routes
    /// back along.
    pub(super) incoming: Referral,
    /// What the seat said.
    pub(super) content: String,
}

/// Fill one planned answer, with no access to the board at all.
///
/// # Errors
///
/// Returns a host-side failure, such as an agent process that did not answer.
pub(super) fn fill_answer(
    seats: &mut [&mut dyn SwarmMember],
    planned: &PlannedAnswer,
) -> Result<SpokenAnswer, String> {
    let content = seats[planned.seat].answer(&planned.incoming, &planned.visible)?;
    Ok(SpokenAnswer {
        desk: planned.desk,
        incoming: planned.incoming.clone(),
        content,
    })
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
    /// Whether this desk was still allowed to route a question when the turn
    /// was planned.
    ///
    /// Carried on the spoken turn rather than looked up beside it: the
    /// concurrent scheduler lands turns from several desks' rounds in one
    /// pass, and pairing them with their plans by position is a correctness
    /// bug waiting for the first time the two lists drift.
    pub(super) budget: bool,
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
        budget: planned.budget,
    })
}
