//! Per-turn machinery: what a turn's audience is, appending it, and running
//! one off-floor exchange round.
//!
//! Everything here is called from [`super::drive_with`]'s step loop, once per
//! authorized turn or once per exchange round between two of them. None of it
//! knows how an episode ends or what its report looks like — that is
//! [`super`]'s job — and none of it tallies anything for scoring, which is
//! [`super::scoring`]'s job. This module answers one question only: given an
//! authorized turn (or a round the library opened between turns), what gets
//! appended to the host's journal and who may read it.

use std::time::{Duration, Instant};

use tinyhivemind_hive::{
    EpisodeState, ExchangePolicy, ExchangeRound, ExchangeState, HiveTurn,
    aside::{AsideDecision, AsideInput, AsidePolicy, Audience, aside},
    dispatch::DispatchConversation,
    exchange,
    mention::{MentionAuthor, resolve as resolve_mentions},
    project_for,
};

use super::scoring::{EpisodeReport, aside_policy};
use super::{ASIDE_MARKER, AsideMode, Host, Participant, Room};

/// The audience one authored line is committed under.
///
/// A line that does not ask for a check is desk-visible, as every line in this
/// harness was before. A line that does is handed to the library's own `aside`
/// fold rather than to a regular expression here: the harness reads the marker
/// to know that a decision is wanted, and the library decides who it may
/// reach. A refusal leaves the row desk-visible, which is the safe direction.
fn audience_for(
    mode: AsideMode,
    host: &Host,
    speaker: &str,
    content: &str,
    policy: AsidePolicy,
) -> Audience {
    if !matches!(
        mode,
        AsideMode::Private | AsideMode::Alongside | AsideMode::OffFloor
    ) || !content.trim_start().starts_with(ASIDE_MARKER)
    {
        return Audience::Desk;
    }
    let roster = host.roster();
    let desks = host.desks();
    let mentions = resolve_mentions(
        content,
        None,
        &MentionAuthor::Agent {
            id: speaker.to_owned(),
        },
        &roster,
        &desks,
    );
    let decision = aside(
        policy,
        &AsideInput {
            conversation: DispatchConversation::from(&host.conversation()),
            author_id: speaker.to_owned(),
            mentions,
            spent: 0,
            unsettled: false,
        },
        &roster,
        &desks,
    );
    match decision {
        Ok(AsideDecision::One { audience }) => audience,
        Ok(AsideDecision::None { .. }) | Err(_) => Audience::Desk,
    }
}

/// Mean rows the room's members were holding when the episode ended.
///
/// Averaged over members rather than summed: the question a window budget asks
/// is what *one participant* has to carry, and a protocol whose cost is
/// "everybody holds everything" is expensive per member precisely because the
/// room is large.
#[expect(
    clippy::cast_precision_loss,
    reason = "a room with more members than an f64 can count is not a room"
)]
pub(super) fn mean_context_rows(agents: &[&mut dyn Participant]) -> f64 {
    if agents.is_empty() {
        return 0.0;
    }
    let total: usize = agents.iter().map(|agent| agent.context_rows()).sum();
    total as f64 / agents.len() as f64
}

/// Fill in the fields only the room can answer, once the episode has ended.
///
/// `drive` stays ignorant of which member is an expert or a decisive
/// hidden-profile holder; only the room knows that, and only after the episode
/// has already decided is it safe to ask.
pub(super) fn scored(room: &Room, report: EpisodeReport) -> EpisodeReport {
    let expert = room.deciding_expert();
    // In time, or not at all. A deposit at or after the commit boundary is
    // compute the room paid for and could not use, and scoring it as a hit
    // would report exactly the failure this arm exists to find.
    let fact_at = expert
        .and_then(|id| {
            report
                .first_deposit
                .iter()
                .find(|(held, _)| held == id)
                .map(|(_, turn)| *turn)
        })
        .filter(|at| report.commit_at.is_none_or(|boundary| *at < boundary));

    let report = EpisodeReport {
        correct: report.decided.as_ref() == Some(&room.truth),
        has_expert: expert.is_some(),
        decisive: expert.map(str::to_owned),
        fact_deposited: fact_at.is_some(),
        fact_at,
        ..report
    };
    debug_check(&report, &room.member_ids(), room);
    report
}

/// Cheap consistency checks over a freshly built report, active only in
/// debug builds.
///
/// These are bookkeeping invariants across the fields `drive` and
/// `run_episode` fill, not library behaviour -- they exist to catch an
/// accounting bug here before it reaches a published number.
fn debug_check(report: &EpisodeReport, member_ids: &[&str], room: &Room) {
    debug_assert!(
        room.planted.as_ref() != Some(&room.truth),
        "a hidden-profile decoy must never be the genuinely best option"
    );
    debug_assert_eq!(
        report
            .speech
            .iter()
            .map(|(_, turns)| u64::from(*turns))
            .sum::<u64>(),
        u64::from(report.turns),
        "speech counts must add up to the turns actually taken"
    );
    debug_assert!(
        report.defers <= report.turns,
        "cannot defer more turns than were taken"
    );
    debug_assert!(
        report.turns == 0 || report.cost_units >= u64::from(report.turns),
        "every turn costs at least one unit"
    );
    debug_assert_eq!(
        report
            .speech
            .iter()
            .map(|(id, count)| u64::from(room.cost_of(id)).saturating_mul(u64::from(*count)))
            .sum::<u64>(),
        report.cost_units,
        "cost_units must equal each speaker's own cost times its turns"
    );
    if let Some(at) = report.fact_at {
        debug_assert!(report.fact_deposited, "fact_at implies fact_deposited");
        debug_assert!(at < report.turns, "fact_at must be a turn actually taken");
        debug_assert!(
            report.commit_at.is_none_or(|boundary| at < boundary),
            "a fact counted as deposited must have landed before the commit boundary"
        );
    } else {
        debug_assert!(!report.fact_deposited, "fact_deposited without fact_at");
    }
    debug_assert!(
        report.knows_turns <= report.turns,
        "cannot hand out more Knows turns than were taken"
    );
    if let Some(proposer) = &report.proposer {
        debug_assert!(
            member_ids.contains(&proposer.as_str()),
            "the proposer must be a member of the room"
        );
    }
}

/// The exchange policy an off-floor arm runs under.
///
/// `contact_cap` is `--aside-cap`, so the off-floor arm and the arms that ride
/// alongside a turn are bounded by the same number and differ in where the row
/// goes rather than in how many there are. `round_cap` is the episode's turn
/// budget, because the harness opens at most one round per turn — so the
/// contact cap is what actually binds, and the round cap is the belt to its
/// braces.
pub(super) fn exchange_policy(contact_cap: u32, round_cap: u32) -> ExchangePolicy {
    ExchangePolicy {
        enabled: true,
        contact_cap,
        round_cap,
    }
}

/// One line of the `--trace` view: who spoke, why, under what visibility and
/// phase, how much they were shown, and what they said.
pub(super) fn trace_line(turn: &HiveTurn, content: &str, saw: usize) -> String {
    format!(
        "{:>10}  {:<10} {:<6} {:<11} saw {saw:>2}  {content}",
        turn.agent_id,
        format!("{:?}", turn.reason).to_lowercase(),
        format!("{:?}", turn.visibility).to_lowercase(),
        format!("{:?}", turn.phase).to_lowercase(),
    )
}

/// Append one authorized turn, and the private row riding alongside it.
///
/// Durably append the turn, then the caller commits the state it returned:
/// that ordering is what the `next_state` contract requires. The aside is
/// appended *after* the floor move, so the desk-visible row is what a reader
/// meets first, and it cannot buy the room a vote — `spent` is already fixed in
/// `turn.next_state`, and `live_traces` drops a non-desk row before it can
/// reach a trace or a standing.
///
/// It does spend a **sequence**, though: the next desk turn lands one raw
/// sequence higher than it would have without the row, and
/// `salience::standing` reads recency as a raw sequence distance. So a private
/// row does perturb which member the attention market hands the floor to next,
/// even though it moves nothing the room counts. See the "Known limitation"
/// note on ADR 0011, and `hive+quiet`, the control that measures it.
pub(super) fn append_turn(
    host: &mut Host,
    turn: &HiveTurn,
    content: String,
    private: Option<String>,
    mode: AsideMode,
    members: usize,
) {
    let policy = aside_policy(members);
    let audience = audience_for(mode, host, &turn.agent_id, &content, policy);
    host.agent_to(&turn.agent_id, content, audience);
    let Some(line) = private else {
        return;
    };
    let audience = audience_for(AsideMode::Alongside, host, &turn.agent_id, &line, policy);
    // A refused audience would put the line on the desk, where it would be a
    // second floor contribution on one turn. The safe direction here is the
    // opposite one: drop it.
    if !audience.is_desk() {
        host.agent_to(&turn.agent_id, line, audience);
    }
}

/// What one exchange round did.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Round {
    /// Members actually asked for a line — the model calls this round paid for,
    /// whether or not the member had anything to say.
    pub(super) calls: u32,
    /// Private rows appended.
    pub(super) rows: u32,
    /// The exchange state to carry into the next round.
    pub(super) next: ExchangeState,
}

/// Ask the library for a round and run it, returning the rows written and the
/// time spent inside the library.
///
/// Split out of [`super::drive_with`] so the step loop stays readable and so
/// the library call is timed the same way [`tinyhivemind_hive::step`] is.
///
/// # Errors
///
/// Returns the library's own error text if a snapshot is malformed.
pub(super) fn one_exchange(
    host: &mut Host,
    agents: &mut [&mut dyn Participant],
    last: &HiveTurn,
    state: &EpisodeState,
    policy: &ExchangePolicy,
    opened: ExchangeState,
    members: usize,
) -> Result<(Round, Duration), String> {
    // The exchange happens *after* the round that authorized `last`, so it
    // reads the journal as it now stands rather than as that round was folded
    // against: its boundary advances to the newest row. Handing `last` through
    // unchanged would withhold from the exchange the very floor rows the round
    // just wrote, which is a boundary for a moment that has passed.
    let last = &HiveTurn {
        round_start: host
            .journal
            .iter()
            .map(|message| message.sequence)
            .max()
            .unwrap_or(last.round_start),
        ..last.clone()
    };
    let started = Instant::now();
    let round = {
        let roster = host.roster();
        let desks = host.desks();
        exchange(policy, state, opened, &host.journal, &roster, &desks)
            .map_err(|error| error.to_string())?
    };
    let mut library = started.elapsed();
    let next = match &round {
        ExchangeRound::Open { next, .. } => *next,
        ExchangeRound::Closed { .. } => opened,
    };
    let ran = exchange_round(
        host,
        agents,
        last,
        &round,
        aside_policy(members),
        &mut library,
    );
    Ok((Round { next, ..ran }, library))
}

/// Run one exchange round, and return how many private rows it wrote.
///
/// Each member the library named is asked for one line, over a projection
/// built at **the visibility the last turn ran under** — so a round during the
/// blind phase still cannot show a member its peers' desk rows, and ADR 0005's
/// blind round is worth exactly what it was worth before. An audience the
/// `aside` fold will not make private is dropped rather than published.
///
/// Every member in the round shares one `round_start`, so **a member in an
/// exchange round cannot read another member's row from that same round**.
/// `ExchangeRound::Open` names all of them at once and ADR 0012 prices the
/// round in model calls, so a host that fanned them out could not have the
/// second read the first; before ADR 0014 this loop leaked that ordering,
/// because it happens to run them one after another. Closing the leak moved
/// `hive+rounds` by 0.1 points in a single cell of the scale sweep, which is
/// recorded rather than reverted.
fn exchange_round(
    host: &mut Host,
    agents: &mut [&mut dyn Participant],
    last: &HiveTurn,
    round: &ExchangeRound,
    policy: AsidePolicy,
    library: &mut Duration,
) -> Round {
    let ExchangeRound::Open { members, .. } = round else {
        return Round::default();
    };
    let mut ran = Round::default();
    for member in members {
        // The turn-holder's own projection, addressed to this member: same
        // visibility, same watermark, this reader's audience.
        let as_member = HiveTurn {
            agent_id: member.clone(),
            ..last.clone()
        };
        let started = Instant::now();
        let visible = project_for(&as_member, &host.journal);
        *library += started.elapsed();
        let Some(agent) = agents.iter_mut().find(|agent| agent.id() == member) else {
            continue;
        };
        // Asked, and therefore paid for, whether or not it answers. This is
        // the number the price column reports: a declined round costs the same
        // calls as a productive one.
        ran.calls = ran.calls.saturating_add(1);
        let Some(line) = agent.exchange(&visible) else {
            continue;
        };
        let started = Instant::now();
        let audience = audience_for(AsideMode::OffFloor, host, member, &line, policy);
        *library += started.elapsed();
        if audience.is_desk() {
            continue;
        }
        host.agent_to(member, line, audience);
        ran.rows = ran.rows.saturating_add(1);
    }
    ran
}
