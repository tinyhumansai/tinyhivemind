//! The episode: a pure state machine over a transcript the caller holds.
//!
//! [`step`] answers one question — who, if anyone, speaks next, and has the
//! room finished — and answers it from arguments alone. There is no port here
//! and nothing to await. A host reads its log through [`SessionLog`], runs the
//! one authorized turn, appends it, and calls [`step`] again.
//!
//! [`SessionLog`]: tinyhivemind::SessionLog

#[cfg(test)]
mod test;

mod types;

pub use types::{
    DEFAULT_REVEALED_WIDTH, DEFAULT_ROUND_WIDTH, EpisodePolicy, EpisodeState, HiveStep, HiveTurn,
    Phase, Visibility,
};

use crate::{
    attention::{AgentThreshold, BidContext, bids, floor_round},
    directory::{Directory, directory, validate_policy as validate_directory_policy},
    error::{Error, Result},
    quorum::{ConsensusState, consensus, standings},
    trace::{TraceKind, read_borrowed},
};
use tinyhivemind::{
    SessionAuthor, SessionMessage, aside::Viewer, desk::DeskSet, project_as, roster::Roster,
};

/// How much a speaker's threshold rises after taking the floor.
const SPEAK_COST: i64 = 500;

/// Decide the next step of a deliberation episode.
///
/// Evaluation order is fixed, and each rung is checked before the next:
///
/// 1. the roster, desk snapshots and policy are validated;
/// 2. a spent budget returns [`HiveStep::Exhausted`];
/// 3. traces, standings and — when `policy.directory` is set — the directory
///    are folded from the transcript, all at the same sequence;
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
/// [`Error::UnknownThresholdMember`] for a threshold naming a non-member,
/// [`Error::ZeroDeferCap`] for `defer_cap: Some(0)`, or a policy error from the
/// quorum, salience and directory folds.
pub fn step(
    state: &EpisodeState,
    transcript: &[SessionMessage],
    roster: &Roster<'_>,
    desks: &DeskSet<'_>,
    policy: &EpisodePolicy,
) -> Result<HiveStep> {
    roster.validate()?;
    desks.validate()?;
    if policy.defer_cap == Some(0) {
        return Err(Error::ZeroDeferCap);
    }
    if policy.round_width == 0 || policy.revealed_width == 0 {
        return Err(Error::ZeroRoundWidth);
    }
    if let Some(directory_policy) = &policy.directory {
        validate_directory_policy(directory_policy)?;
    }
    let members = active_members(roster, desks, state)?;

    if state.spent >= policy.turn_budget {
        return Ok(HiveStep::Exhausted { spent: state.spent });
    }

    let (live, traces, at) = live_traces(transcript, state, &members);
    let standings = standings(&traces, at, &policy.quorum)?;

    let consensus = consensus(&standings, &policy.quorum);
    match &consensus {
        ConsensusState::Quorum { topic } => {
            if let Some(standing) = converged_standing(state, &traces, &standings, topic) {
                return Ok(HiveStep::Converged {
                    topic: topic.clone(),
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
            if !has_free_dissenter(&members, &standings, topics) {
                return Ok(HiveStep::Deadlocked {
                    topics: topics.clone(),
                });
            }
        }
        ConsensusState::Deliberating => {}
    }

    // The directory is folded at the same sequence as the standings, so the
    // bid reads one consistent view of the transcript rather than two.
    let known = match &policy.directory {
        Some(directory_policy) => {
            Some(directory(&traces, at, directory_policy, &state.thresholds)?)
        }
        None => None,
    };
    let context = context(
        &traces,
        &standings,
        &members,
        state,
        policy,
        at,
        known.as_ref(),
    );
    let bids = bids(&context)?;
    let phase = if matches!(consensus, ConsensusState::Quorum { .. }) {
        Phase::Commit
    } else {
        state.phase
    };
    Ok(authorized(
        state,
        policy,
        &bids,
        &members,
        Round {
            phase,
            at,
            visibility: visibility(policy, &live, &members),
            unheard: unheard(&live, &members),
        },
    ))
}

/// What the round being authorized already knows about itself, before its
/// members are picked.
///
/// Three values [`step`] has computed and [`authorized`] needs, grouped so the
/// hand-off is one argument rather than three positional ones of the same
/// shape.
struct Round<'a> {
    /// Which class of turn the round is taking.
    phase: Phase,
    /// The sequence the round was folded at, and so its boundary.
    at: tinyhivemind::Sequence,
    /// How much of the transcript every turn in the round may see.
    visibility: Visibility,
    /// Members that have not yet authored a turn this episode. Its length is
    /// how many turns the blind round still has left to run, and the round
    /// picks from these identities rather than merely capping at their
    /// count — a heard member's bid must not crowd out one still owed a turn.
    unheard: Vec<&'a str>,
}

/// Pick the round's members and build the state the episode takes after it.
///
/// Split out of [`step`] at the seam between *deciding whether the episode
/// continues* and *authorizing who speaks*: everything above is a termination
/// check, everything here is a round.
fn authorized(
    state: &EpisodeState,
    policy: &EpisodePolicy,
    bids: &[crate::attention::Bid],
    members: &[&str],
    round: Round,
) -> HiveStep {
    // The room records one decision, so a commit round is one turn wide
    // however wide the policy allows. Widening it would let two members record
    // different decisions for the same episode.
    //
    // The rest of the width is bounded three ways at once, and the narrowest
    // wins: the policy's `round_width`, how many members actually cleared
    // their threshold, and how much of `turn_budget` is left. The last is what
    // keeps the budget a bound on *turns* rather than on rounds.
    let remaining = policy.turn_budget.saturating_sub(state.spent);
    // Blind and revealed rounds are bounded separately because they cost
    // different things: a blind member cannot read a peer's row whether or not
    // the round is concurrent, so widening there is free, while a revealed
    // member's turn depends on exactly the row a concurrent peer is writing.
    let cap = match round.visibility {
        // Never past the end of the blind round. Authorizing more would spend
        // turns on members already heard and carry the round past the boundary
        // the blind phase ends at, which is what makes a wide blind round
        // *free* rather than merely cheap: the same turns, the same
        // projections, fewer waits.
        Visibility::Blind => policy
            .round_width
            .min(u32::try_from(round.unheard).unwrap_or(u32::MAX))
            .max(1),
        Visibility::Full => policy.revealed_width,
    };
    let width = if round.phase == Phase::Commit {
        1
    } else {
        cap.min(remaining)
    };
    let speaking = floor_round(bids, width);
    if speaking.is_empty() {
        return HiveStep::Idle;
    }

    let speakers: Vec<&str> = speaking.iter().map(|bid| bid.agent_id.as_str()).collect();
    // `speaking.len() <= remaining` by the clamp above, so this cannot exceed
    // `turn_budget` and cannot saturate; the saturating form keeps the
    // arithmetic total without an unreachable error branch.
    let spent = state
        .spent
        .saturating_add(u32::try_from(speaking.len()).unwrap_or(u32::MAX));

    let turns = speaking
        .iter()
        .map(|bid| HiveTurn {
            agent_id: bid.agent_id.clone(),
            phase: round.phase,
            visibility: round.visibility,
            reason: bid.reason,
            watermark: state.watermark,
            round_start: round.at,
        })
        .collect();

    HiveStep::Speak {
        turns,
        next_state: Box::new(EpisodeState {
            conversation: state.conversation.clone(),
            spent,
            phase: round.phase,
            thresholds: charged(&state.thresholds, members, &speakers),
            watermark: state.watermark,
            commit_boundary: next_commit_boundary(state, round.phase, round.at),
        }),
    }
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
///
/// A trace deposited in a row addressed to an aside is dropped for the same
/// reason and by the same test: **an aside carries information, never
/// support.** The filter is uniform rather than per-reader — every participant
/// counts the same medium, so quorum, the floor and the directory stay
/// single-valued — and it is what makes a private exchange owe the room a
/// settlement. To make an aside count, a member spends a desk-visible turn
/// saying so in the open. See [ADR 0010][adr].
///
/// [adr]: https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/adr/0010-an-aside-carries-information-never-support.md
fn live_traces<'a>(
    transcript: &'a [SessionMessage],
    state: &EpisodeState,
    members: &[&str],
) -> (
    Vec<&'a SessionMessage>,
    Vec<crate::trace::Trace>,
    tinyhivemind::Sequence,
) {
    let live: Vec<&SessionMessage> = transcript
        .iter()
        .filter(|message| message.sequence > state.watermark)
        .filter(|message| message.audience.is_desk())
        .filter(|message| match &message.author {
            SessionAuthor::Agent { id, .. } => members.iter().any(|member| *member == id),
            SessionAuthor::Operator
            | SessionAuthor::Person { .. }
            | SessionAuthor::System { .. } => true,
        })
        .collect();
    let traces = read_borrowed(&live);
    let at = live
        .last()
        .map_or(state.watermark, |message| message.sequence);
    (live, traces, at)
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
            topics.contains(&standing.topic)
                && standing.supporters.iter().any(|held| held == member)
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
    at: tinyhivemind::Sequence,
) -> Option<tinyhivemind::Sequence> {
    if phase == Phase::Commit {
        Some(state.commit_boundary.unwrap_or(at))
    } else {
        None
    }
}

/// Filter a transcript to what one authorized turn may see.
///
/// Two filters compose here, and they narrow along different axes.
///
/// **Visibility**, by time. Under [`Visibility::Full`] every message passes.
/// Under [`Visibility::Blind`] the messages of *peer agents authored within
/// this episode* are withheld, while operator, person and system messages, the
/// turn-holder's own, and anything at or below the episode's watermark
/// remain — the participant still sees the task, its own work, and the
/// conversation that led into the episode, just not the positions its peers
/// have taken since the room opened. A message withheld this way is **dropped**:
/// the round ends, and it arrives on the next turn.
///
/// **Audience**, by addressee. A row addressed to an aside the turn-holder is
/// not part of is **elided rather than dropped**, and a run of them from one
/// aside collapses into a single stub. It keeps its sequence, its author and
/// its audience, and loses only its content, so a citation still resolves and
/// the turn-holder can tell that a peer knows something it does not.
///
/// `transcript` is expected to be the true medium — projected for an operator,
/// or otherwise unelided. Narrowing it beforehand would not make this function
/// wrong, but it would mean [`step`] had folded a transcript that is nobody's
/// room, which is the one thing the episode may not do.
#[must_use]
pub fn project_for(turn: &HiveTurn, messages: &[SessionMessage]) -> Vec<SessionMessage> {
    let viewer = Viewer::Agent {
        id: turn.agent_id.clone(),
    };
    let visible: Vec<SessionMessage> = messages
        .iter()
        .filter(|message| readable(turn, message))
        .cloned()
        .collect();
    project_as(&visible, &viewer)
}

/// Whether one message is inside this turn's two time filters.
///
/// A peer agent's row is withheld when it was authored after the round was
/// folded — it is *concurrent* with this turn, so this turn cannot have read
/// it — and additionally, while the room is blind, when it was authored
/// anywhere within the episode. The turn-holder's own rows, and everything
/// that is not a peer agent, pass either way.
fn readable(turn: &HiveTurn, message: &SessionMessage) -> bool {
    let SessionAuthor::Agent { id, .. } = &message.author else {
        return true;
    };
    if id == &turn.agent_id {
        return true;
    }
    match turn.visibility {
        // Concurrent rows, floor or private alike. A round authorizes turns
        // that must decide exactly as they would arriving one at a time, and
        // a private row an earlier speaker appends *during* the round is just
        // as concurrent as a floor row would be — an audience match does not
        // make it any less written mid-round. So the cutoff applies uniformly
        // to every peer-authored row rather than exempting private ones; only
        // audience elision (applied afterward, by `project_as`) tells a
        // member of the aside from an outsider.
        //
        // This does not let a private row move the round boundary itself:
        // `round_start` is folded from desk rows only (`live_traces`), so
        // appending an aside cannot shift it, and the addition invariant in
        // `tests/fuzz_invariants.rs` still holds. It only means a private row
        // above that boundary is as withheld as a floor row would be.
        //
        // At `round_width: 1` nothing is above `round_start` when the turn
        // composes, so this withholds nothing and a round of one is
        // bit-identical to the sequential episode.
        Visibility::Full => message.sequence <= turn.round_start,
        // The whole episode. `round_start` is the last desk row folded and
        // every folded row is above the watermark, so the watermark is always
        // the stricter of the two — a blind turn reads no peer row authored
        // this episode, concurrent or not.
        Visibility::Blind => message.sequence <= turn.watermark,
    }
}

fn context<'a>(
    traces: &'a [crate::trace::Trace],
    standings: &'a [crate::quorum::TopicStanding],
    members: &'a [&'a str],
    state: &'a EpisodeState,
    policy: &'a EpisodePolicy,
    at: tinyhivemind::Sequence,
    known: Option<&'a Directory>,
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
        quorum: &policy.quorum,
        directory: known,
        directory_policy: policy.directory.as_ref(),
        defer_cap: policy.defer_cap,
    }
}

/// The opening round is blind until every member has been heard once.
///
/// "Heard" means *authored a live turn*, not *deposited a trace*. A member
/// that speaks plain prose with no `!marker` still took its turn; counting
/// only trace authors would keep a room blind forever if any member never
/// happens to cast a formal vote.
fn visibility(policy: &EpisodePolicy, live: &[&SessionMessage], members: &[&str]) -> Visibility {
    if unheard(live, members) == 0 || !policy.blind_round {
        Visibility::Full
    } else {
        Visibility::Blind
    }
}

/// How many members have not yet authored a live turn this episode.
///
/// The blind round is exactly this many turns long, so it is also the width a
/// blind round may usefully take: authorizing more would spend turns on
/// members that have already been heard, and end the blind round somewhere
/// past its own boundary. "Heard" means *authored a live turn*, not
/// *deposited a trace* — a member that speaks plain prose with no `!marker`
/// still took its turn, and counting only trace authors would keep a room
/// blind forever if any member never happens to cast a formal vote.
fn unheard(live: &[&SessionMessage], members: &[&str]) -> usize {
    let heard = members
        .iter()
        .filter(|member| {
            live.iter().any(|message| match &message.author {
                SessionAuthor::Agent { id, .. } => id == **member,
                SessionAuthor::Operator
                | SessionAuthor::Person { .. }
                | SessionAuthor::System { .. } => false,
            })
        })
        .count();
    members.len().saturating_sub(heard)
}

/// Raise the speaker's threshold and lower everyone else's.
///
/// Speaking costs; silence accrues standing. That rotates the floor around the
/// desk rather than fixing a priority order. It is deliberately *not*
/// specialisation: only `threshold` moves, the same amount whatever the turn
/// was about, and `affinity` is host-supplied and never written here. The
/// per-topic estimate earned from the transcript is
/// [`directory`](crate::directory::directory), which is folded fresh on every
/// step rather than carried in state.
fn charged(
    thresholds: &[AgentThreshold],
    members: &[&str],
    speakers: &[&str],
) -> Vec<AgentThreshold> {
    members
        .iter()
        .map(|member| {
            let mut record = thresholds
                .iter()
                .find(|held| held.agent_id == *member)
                .cloned()
                .unwrap_or_else(|| AgentThreshold::new(*member, 0));
            // Every speaker in the round is charged once and everyone silent
            // through it accrues once, whatever the round's width. Charging
            // per round rather than per turn would make a wide round cheap and
            // rotate the floor slower the more concurrent it got.
            record.threshold = if speakers.contains(member) {
                record.threshold.saturating_add(SPEAK_COST)
            } else {
                record.threshold.saturating_sub(SPEAK_COST / 2)
            };
            record
        })
        .collect()
}
