//! The two ways a federation can take one pass over its desks.
//!
//! A pass gives every unfinished desk the chance to do one thing: answer a
//! referral that arrived, put a bounded question to another channel, or fill
//! the one turn its own episode authorized. What differs between the two
//! schedulers here is **when the model is called**, and — as a consequence
//! that is not obvious and is the reason this module carries a warning rather
//! than a note — **when a routed referral is delivered**.
//!
//! # They are not equivalent, and the difference is not a rounding error
//!
//! [`sequential_pass`] walks the desks in order and finishes each one before
//! starting the next. So a question routed from desk 3 to desk 7 lands in desk
//! 7's queue *before the pass reaches desk 7*, and desk 7 answers it that same
//! pass, spending the pass on the answer instead of on a turn.
//!
//! [`concurrent_pass`] cannot do that and still be concurrent. It settles what
//! every desk owes, plans every desk's turn, runs every desk's model call at
//! once, and only then lands the rows — so a question routed while landing is
//! delivered on the *next* pass. The rows are the same rows and the routing is
//! the same routing; they arrive one pass later.
//!
//! One pass of latency changes the sequence number a row lands at, and both
//! `QuorumPolicy::window` and salience decay read a *raw* sequence distance.
//! Measured on the recorded three-desk federation at 200 samples, that is the
//! difference between `swarm 78.0` and `swarm 67.0`. Neither is wrong; they are
//! two schedules, and a host may write either.
//!
//! **So the sequential pass is the reference.** It is what every recorded swarm
//! number in `docs/` and on the wiki was taken against, it is what a simulated
//! run uses, and it is what `--jobs 1` selects. The concurrent pass exists for
//! one situation, where it is worth a schedule of its own: a **live**
//! federation, where a turn is a model call of several seconds and a hundred
//! desks each authorizing exactly one speaker is a hundred calls that need not
//! wait on each other.
//!
//! # What concurrency does not relax
//!
//! One message, one turn is untouched. Each desk still runs exactly one turn
//! per pass, authorized by its own episode's `step`; what overlaps is the
//! *waiting*, across episodes that were already independent. That is precisely
//! the arrangement [ADR 0002][adr] carves out — "a host that needs parallel
//! speculative work must express it as separate episodes and reconcile them
//! itself" — and every desk here is a separate episode with its own journal,
//! its own state and its own budget.
//!
//! Rows are landed in **desk order** whatever order the calls return, so the
//! schedule is deterministic: the same seed and the same `--jobs` produce the
//! same transcript.
//!
//! [adr]: https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/adr/0002-hive-episodes-are-sequential.md

use std::time::Instant;

use tinyhivemind_hive::{EpisodePolicy, EpisodeState, HiveStep, step};

use super::board::{Board, PlannedTurn, fill_turn};
use super::{Channel, DeskOutcome, Ending, SwarmMember, member};
use crate::parallel;

/// One pass, desk by desk, each finished before the next begins.
///
/// Returns whether anything happened; a pass in which nothing did is what ends
/// the run.
///
/// # Errors
///
/// Returns the library's own error text, or a host-side agent failure.
pub(super) fn sequential_pass(
    board: &mut Board<'_>,
    members: &mut [Vec<&mut dyn SwarmMember>],
    channels: &[Channel],
    states: &mut [EpisodeState],
    finished: &mut [Option<DeskOutcome>],
    policy: &EpisodePolicy,
) -> Result<bool, String> {
    let mut progressed = false;
    for desk in 0..channels.len() {
        if finished[desk].is_some() {
            board.strand(desk);
            continue;
        }
        if let Some(incoming) = board.pop_pending(desk) {
            board.deliver(members, desk, &incoming)?;
            progressed = true;
            continue;
        }
        // Off the floor, and so before the turn rather than instead of it: the
        // desk puts its bounded question to another channel and still has
        // every turn its own size earned. Bounded by the ask cap, so this
        // cannot keep the loop alive on its own.
        if board.ask_off_floor(members, desk)? {
            progressed = true;
            continue;
        }

        let decision = decide(board, states, policy, desk)?;
        match decision {
            HiveStep::Speak { turn } => {
                let planned = board.plan_turn(members, desk, &turn)?;
                let said = {
                    let seats = &mut members[desk];
                    fill_turn(seats, &planned)?
                };
                board.land_turn(members, &said, planned.budget)?;
                states[desk] = turn.next_state;
            }
            ended => retire(channels, finished, desk, ended),
        }
        progressed = true;
    }
    Ok(progressed)
}

/// One pass in four phases, with every desk's model call in flight at once.
///
/// Read the module documentation before choosing this over
/// [`sequential_pass`]: it delivers a routed referral one pass later, and that
/// is observable in what the federation decides.
///
/// # Errors
///
/// Returns the library's own error text, or a host-side agent failure.
pub(super) fn concurrent_pass(
    board: &mut Board<'_>,
    members: &mut [Vec<&mut dyn SwarmMember>],
    channels: &[Channel],
    states: &mut [EpisodeState],
    finished: &mut [Option<DeskOutcome>],
    policy: &EpisodePolicy,
    jobs: usize,
) -> Result<bool, String> {
    let count = channels.len();
    let mut progressed = false;

    // Phase one, sequential and cheap: settle what every desk owes before
    // anybody speaks. A pending answer and an off-floor question are both
    // desk-local and both rare next to turns.
    for (desk, outcome) in finished.iter().enumerate() {
        if outcome.is_some() {
            board.strand(desk);
            continue;
        }
        if let Some(incoming) = board.pop_pending(desk) {
            board.deliver(members, desk, &incoming)?;
            progressed = true;
            continue;
        }
        if board.ask_off_floor(members, desk)? {
            progressed = true;
        }
    }

    // Phase two, sequential and cheap: ask the library who speaks next on each
    // desk, and retire the desks that are done.
    let mut planned: Vec<PlannedTurn> = Vec::new();
    for desk in 0..count {
        if finished[desk].is_some() || !board.pending_empty(desk) {
            continue;
        }
        match decide(board, states, policy, desk)? {
            HiveStep::Speak { turn } => planned.push(board.plan_turn(members, desk, &turn)?),
            ended => {
                retire(channels, finished, desk, ended);
                progressed = true;
            }
        }
    }

    // An index from desk to its planned turn, so the seats and the plans can
    // be walked together in one pass. Built rather than searched: at a hundred
    // desks a scan per desk is a hundred scans, for a lookup that is a
    // subscript.
    let mut plan_at: Vec<Option<usize>> = vec![None; count];
    for (index, plan) in planned.iter().enumerate() {
        plan_at[plan.desk] = Some(index);
    }

    // Phase three, concurrent: every desk fills its one authorized turn at the
    // same time.
    let spoken = {
        let mut work: Vec<(&PlannedTurn, &mut Vec<&mut dyn SwarmMember>)> = members
            .iter_mut()
            .enumerate()
            .filter_map(|(desk, seats)| {
                plan_at
                    .get(desk)
                    .copied()
                    .flatten()
                    .map(|index| (&planned[index], seats))
            })
            .collect();
        parallel::map_mut_in_order(&mut work, jobs, |(plan, seats)| fill_turn(seats, plan))?
    };

    // Phase four, sequential and in desk order. The order is load-bearing
    // rather than tidy: a row consumes a sequence number, and the quorum
    // window and salience decay read raw sequence distance, so landing in
    // completion order would let one seed decide two different things.
    for said in &spoken {
        let Some(plan) = plan_at
            .get(said.desk)
            .copied()
            .flatten()
            .and_then(|index| planned.get(index))
        else {
            continue;
        };
        board.land_turn(members, said, plan.budget)?;
        states[said.desk] = plan.turn.next_state.clone();
        progressed = true;
    }

    Ok(progressed)
}

/// Ask the library what one desk does next, charging the time to the report.
fn decide(
    board: &mut Board<'_>,
    states: &[EpisodeState],
    policy: &EpisodePolicy,
    desk: usize,
) -> Result<HiveStep, String> {
    let started = Instant::now();
    let decision = {
        let host = board.host();
        let roster = host.roster();
        let desk_set = host.desks();
        step(
            &states[desk],
            &host.journals[desk],
            &roster,
            &desk_set,
            policy,
        )
    };
    board.add_library_time(started.elapsed());
    decision.map_err(|error| error.to_string())
}

/// Record how a desk's episode ended.
///
/// `Speak` never reaches here — it is handled by the caller, which is the one
/// that holds the seats — so it is treated as the impossibility it is rather
/// than given a fabricated ending.
fn retire(
    channels: &[Channel],
    finished: &mut [Option<DeskOutcome>],
    desk: usize,
    ended: HiveStep,
) {
    let outcome = match ended {
        HiveStep::Converged { topic, .. } => {
            member::outcome(channels, desk, Ending::Converged, Some(topic))
        }
        HiveStep::Deadlocked { .. } => member::outcome(channels, desk, Ending::Deadlocked, None),
        HiveStep::Exhausted { .. } => member::outcome(channels, desk, Ending::Exhausted, None),
        HiveStep::Idle | HiveStep::Speak { .. } => {
            member::outcome(channels, desk, Ending::Idle, None)
        }
    };
    finished[desk] = Some(outcome);
}
