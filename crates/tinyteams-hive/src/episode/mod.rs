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
    trace::{TraceKind, read},
};
use tinyteams::{SessionAuthor, SessionMessage, desk::DeskSet, roster::Roster};

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
    let members = active_members(roster, desks, state)?;

    if state.spent >= policy.turn_budget {
        return Ok(HiveStep::Exhausted { spent: state.spent });
    }

    let (traces, at) = live_traces(transcript, state, &members);
    let standings = standings(&traces, at, &policy.quorum)?;

    match consensus(&standings, &policy.quorum) {
        ConsensusState::Quorum { topic } => {
            if let Some(standing) = converged_standing(state, &traces, &standings, &topic) {
                return Ok(HiveStep::Converged {
                    topic,
                    standing: Box::new(standing),
                });
            }
        }
        ConsensusState::Deadlocked { topics } => {
            // A deadlock is only terminal once nobody is left to break it. A
            // member that has backed neither tied topic is free to break the
            // tie — whether or not that member is the one who wins the floor
            // this time. This is checked directly against the standings
            // rather than through a bid's `reason`: a member who has also been
            // cited or objected to is classified `Addressed` ahead of
            // `Dissent` by bid precedence, which would otherwise mask a real
            // dissenter and let the episode terminate early.
            if !has_free_dissenter(&members, &standings, &topics) {
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
    let commit_boundary = next_commit_boundary(state, phase, at);
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
                commit_boundary,
            },
        }),
    })
}

/// Resolve the episode's desk to its current, active member ids.
///
/// # Errors
///
/// Returns [`Error::UnknownThresholdMember`] when a carried threshold names
/// someone who is no longer an active member of this desk.
fn active_members<'a>(
    roster: &Roster<'_>,
    desks: &DeskSet<'a>,
    state: &EpisodeState,
) -> Result<Vec<&'a str>> {
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
    Ok(members)
}

/// Fold the transcript above the watermark into traces, and the sequence
/// standings should be computed at.
///
/// A trace is only a vote if its author is a current member of this desk.
/// Without this filter a retired agent, or one from a different desk, whose
/// message lands after the watermark would still be folded into standings
/// and could manufacture quorum nobody eligible actually holds.
fn live_traces(
    transcript: &[SessionMessage],
    state: &EpisodeState,
    members: &[&str],
) -> (Vec<crate::trace::Trace>, tinyteams::Sequence) {
    let live: Vec<SessionMessage> = transcript
        .iter()
        .filter(|message| message.sequence > state.watermark)
        .filter(|message| match &message.author {
            SessionAuthor::Agent { id, .. } => members.iter().any(|member| *member == id),
            SessionAuthor::Operator
            | SessionAuthor::Person { .. }
            | SessionAuthor::System { .. } => true,
        })
        .cloned()
        .collect();
    let traces = read(&live);
    let at = live
        .last()
        .map_or(state.watermark, |message| message.sequence);
    (traces, at)
}

/// The standing to converge on, if the commit turn actually recorded it.
///
/// `consensus` names a topic it found in `standings`, so the lookup here
/// cannot miss; it is written as a search rather than an unwrap so there is
/// no panicking path in library code.
fn converged_standing(
    state: &EpisodeState,
    traces: &[crate::trace::Trace],
    standings: &[crate::quorum::TopicStanding],
    topic: &crate::trace::TopicId,
) -> Option<crate::quorum::TopicStanding> {
    if state.phase != Phase::Commit {
        return None;
    }
    let boundary = state.commit_boundary?;
    let recorded = traces.iter().any(|trace| {
        trace.kind == TraceKind::Commit
            && trace.topic.as_ref() == Some(topic)
            && trace.sequence > boundary
    });
    if !recorded {
        return None;
    }
    standings
        .iter()
        .find(|standing| &standing.topic == topic)
        .cloned()
}

/// Whether a member remains who has backed neither tied topic.
fn has_free_dissenter(
    members: &[&str],
    standings: &[crate::quorum::TopicStanding],
    topics: &[crate::trace::TopicId],
) -> bool {
    members.iter().any(|member| {
        !standings.iter().any(|standing| {
            topics.contains(&standing.topic) && standing.supporters.iter().any(|held| held == member)
        })
    })
}

/// Fix the commit boundary the moment the phase first flips to `Commit`, at
/// the sequence standings were folded to for that decision, and carry it
/// unchanged for the rest of the episode -- the boundary a converging
/// `!commit` trace must land strictly after.
fn next_commit_boundary(
    state: &EpisodeState,
    phase: Phase,
    at: tinyteams::Sequence,
) -> Option<tinyteams::Sequence> {
    if phase == Phase::Commit {
        Some(state.commit_boundary.unwrap_or(at))
    } else {
        None
    }
}

/// Filter a transcript to what one authorized turn may see.
///
/// Under [`Visibility::Full`] every message is visible. Under
/// [`Visibility::Blind`] the messages of *peer agents authored within this
/// episode* are withheld, while operator, person and system messages, the
/// turn-holder's own, and anything at or below the episode's watermark
/// remain — the participant still sees the task, its own work, and the
/// conversation that led into the episode, just not the positions its peers
/// have taken since the room opened.
#[must_use]
pub fn project_for<'a>(turn: &HiveTurn, messages: &'a [SessionMessage]) -> Vec<&'a SessionMessage> {
    let watermark = turn.next_state.watermark;
    messages
        .iter()
        .filter(|message| match turn.visibility {
            Visibility::Full => true,
            Visibility::Blind => match &message.author {
                SessionAuthor::Agent { id, .. } => {
                    id == &turn.agent_id || message.sequence <= watermark
                }
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
fn charged(thresholds: &[AgentThreshold], members: &[&str], speaker: &str) -> Vec<AgentThreshold> {
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
