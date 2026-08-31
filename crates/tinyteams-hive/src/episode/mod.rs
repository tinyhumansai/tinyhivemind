//! The episode: a pure state machine over a transcript the caller holds.
//!
//! [`step`] answers one question — who, if anyone, speaks next, and has the
//! room finished — and answers it from arguments alone. There is no port here
//! and nothing to await. A host reads its log through [`SessionLog`], runs the
//! one authorized turn, appends it, and calls [`step`] again.
//!
//! [`SessionLog`]: tinyteams::SessionLog

#[cfg(test)]
mod test;

mod types;

pub use types::{EpisodePolicy, EpisodeState, HiveStep, HiveTurn, Phase, Visibility};

use crate::{
    attention::{AgentThreshold, BidContext, bids, floor_holder},
    error::{Error, Result},
    quorum::{ConsensusState, consensus, standings},
    trace::read,
};
use tinyteams::{
    SessionAuthor, SessionMessage,
    desk::DeskSet,
    roster::Roster,
};

/// How much a speaker's threshold rises after taking the floor.
const SPEAK_COST: i64 = 500;

/// Decide the next step of a deliberation episode.
///
/// Evaluation order is fixed, and each rung is checked before the next:
///
/// 1. the roster and desk snapshots are validated;
/// 2. a spent budget returns [`HiveStep::Exhausted`];
/// 3. traces and standings are folded from the transcript;
/// 4. quorum in [`Phase::Commit`] returns [`HiveStep::Converged`];
/// 5. quorum in [`Phase::Deliberate`] flips the phase and emits one commit turn;
/// 6. a deadlock nobody can break returns [`HiveStep::Deadlocked`];
/// 7. otherwise the highest bid takes the floor, or [`HiveStep::Idle`].
///
/// `transcript` is the projection of the episode's conversation. Messages at or
/// below `state.watermark` are context and are not folded into traces, so an
/// episode does not inherit the votes of the conversation that preceded it.
///
/// # Errors
///
/// Returns [`Error::Core`] for a malformed roster or desk snapshot,
/// [`Error::UnknownThresholdMember`] for a threshold naming a non-member, or a
/// policy error from the quorum and salience folds.
pub fn step(
    state: &EpisodeState,
    transcript: &[SessionMessage],
    roster: &Roster<'_>,
    desks: &DeskSet<'_>,
    policy: &EpisodePolicy,
) -> Result<HiveStep> {
    roster.validate()?;
    desks.validate()?;

    let desk_id = desks.resolve_id(&state.conversation.desk_id)?;
    let members: Vec<&str> = desks
        .members(desk_id)?
        .into_iter()
        .filter(|id| roster.active_member(id).is_some())
        .collect();
    for threshold in &state.thresholds {
        if !members.iter().any(|id| *id == threshold.agent_id) {
            return Err(Error::UnknownThresholdMember {
                agent_id: threshold.agent_id.clone(),
                desk_id: desk_id.to_owned(),
            });
        }
    }

    if state.spent >= policy.turn_budget {
        return Ok(HiveStep::Exhausted { spent: state.spent });
    }

    let live: Vec<SessionMessage> = transcript
        .iter()
        .filter(|message| message.sequence > state.watermark)
        .cloned()
        .collect();
    let traces = read(&live);
    let at = live
        .last()
        .map_or(state.watermark, |message| message.sequence);
    let standings = standings(&traces, at, &policy.quorum)?;

    match consensus(&standings, &policy.quorum) {
        ConsensusState::Quorum { topic } => {
            // `consensus` names a topic it found in `standings`, so the lookup
            // below cannot miss; it is written as a match rather than an
            // unwrap so there is no panicking path in library code.
            if state.phase == Phase::Commit
                && let Some(standing) = standings
                    .iter()
                    .find(|standing| standing.topic == topic)
                    .cloned()
            {
                return Ok(HiveStep::Converged {
                    topic,
                    standing: Box::new(standing),
                });
            }
        }
        ConsensusState::Deadlocked { topics } => {
            // A deadlock is only terminal once nobody is left to break it. A
            // member that has backed neither side bids `Dissent`, and while one
            // exists the room gets another turn — whether or not that member is
            // the one who wins the floor this time.
            let context = context(&traces, &standings, &members, state, policy, at);
            let free = bids(&context)?
                .iter()
                .any(|bid| bid.reason == crate::attention::BidReason::Dissent);
            if !free {
                return Ok(HiveStep::Deadlocked { topics });
            }
        }
        ConsensusState::Deliberating => {}
    }

    let context = context(&traces, &standings, &members, state, policy, at);
    let bids = bids(&context)?;
    let Some(bid) = floor_holder(&bids) else {
        return Ok(HiveStep::Idle);
    };

    let phase = if matches!(
        consensus(&standings, &policy.quorum),
        ConsensusState::Quorum { .. }
    ) {
        Phase::Commit
    } else {
        state.phase
    };
    // `spent < turn_budget <= u32::MAX` was established above, so this
    // addition cannot saturate; the saturating form is used only to keep the
    // arithmetic total without an unreachable error branch.
    let spent = state.spent.saturating_add(1);

    Ok(HiveStep::Speak {
        turn: Box::new(HiveTurn {
            agent_id: bid.agent_id.clone(),
            phase,
            visibility: visibility(policy, &traces, &members),
            reason: bid.reason,
            next_state: EpisodeState {
                conversation: state.conversation.clone(),
                spent,
                phase,
                thresholds: charged(&state.thresholds, &members, &bid.agent_id),
                watermark: state.watermark,
            },
        }),
    })
}

/// Filter a transcript to what one authorized turn may see.
///
/// Under [`Visibility::Full`] every message is visible. Under
/// [`Visibility::Blind`] the messages of *peer agents* are withheld, while
/// operator, person and system messages and the turn-holder's own remain — the
/// participant still sees the task and its own work, just not the positions it
/// would otherwise anchor on.
#[must_use]
pub fn project_for<'a>(turn: &HiveTurn, messages: &'a [SessionMessage]) -> Vec<&'a SessionMessage> {
    messages
        .iter()
        .filter(|message| match turn.visibility {
            Visibility::Full => true,
            Visibility::Blind => match &message.author {
                SessionAuthor::Agent { id, .. } => id == &turn.agent_id,
                SessionAuthor::Operator
                | SessionAuthor::Person { .. }
                | SessionAuthor::System { .. } => true,
            },
        })
        .collect()
}

fn context<'a>(
    traces: &'a [crate::trace::Trace],
    standings: &'a [crate::quorum::TopicStanding],
    members: &'a [&'a str],
    state: &'a EpisodeState,
    policy: &'a EpisodePolicy,
    at: tinyteams::Sequence,
) -> BidContext<'a> {
    BidContext {
        traces,
        standings,
        members,
        thresholds: &state.thresholds,
        at,
        weights: &policy.weights,
        dominance_cap: policy.dominance_cap,
        repetition_cap: policy.repetition_cap,
        window: policy.quorum.window,
    }
}

/// The opening round is blind until every member has been heard once.
fn visibility(
    policy: &EpisodePolicy,
    traces: &[crate::trace::Trace],
    members: &[&str],
) -> Visibility {
    if !policy.blind_round {
        return Visibility::Full;
    }
    let heard = members
        .iter()
        .filter(|member| {
            traces
                .iter()
                .any(|trace| trace.agent_id() == Some(**member))
        })
        .count();
    if heard < members.len() {
        Visibility::Blind
    } else {
        Visibility::Full
    }
}

/// Raise the speaker's threshold and lower everyone else's.
///
/// Speaking costs; silence accrues standing. That is what turns two scalars per
/// member into emergent specialisation rather than a fixed priority order.
fn charged(
    thresholds: &[AgentThreshold],
    members: &[&str],
    speaker: &str,
) -> Vec<AgentThreshold> {
    members
        .iter()
        .map(|member| {
            let mut record = thresholds
                .iter()
                .find(|held| held.agent_id == *member)
                .cloned()
                .unwrap_or_else(|| AgentThreshold::new(*member, 0));
            record.threshold = if *member == speaker {
                record.threshold.saturating_add(SPEAK_COST)
            } else {
                record.threshold.saturating_sub(SPEAK_COST / 2)
            };
            record
        })
        .collect()
}
