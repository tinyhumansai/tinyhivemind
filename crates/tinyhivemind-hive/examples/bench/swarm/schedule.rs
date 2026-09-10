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
//! # Two widths, and neither is this module's to widen
//!
//! Concurrency here is now two-dimensional, and the two halves belong to
//! different owners.
//!
//! **Within a desk**, the library decides. [`HiveStep::Speak`] carries a round
//! of up to `policy.round_width` turns, and [ADR 0014][round] is what says a
//! round may run concurrently: members writing simultaneously cannot read each
//! other, so a concurrent round *is* a blind round. This module runs the round
//! it is handed and never widens one.
//!
//! **Across desks**, the host decides, and that is what this module is. Every
//! desk is a separate episode with its own journal, its own state and its own
//! budget, so overlapping them overlaps only the *waiting*.
//!
//! `--jobs` bounds **desks in flight, not calls**. Each worker takes one
//! desk's whole job — its round, or the answer it owes — and fills it with a
//! sequential iterator, so at most `jobs` model calls are outstanding at once.
//! `desks x round_width` is the number of calls a pass makes, which is a
//! different quantity and not the concurrency width. Widening a round shortens
//! the *pass* — fewer passes to hear everybody — rather than putting more
//! calls in the air at a time.
//!
//! What is preserved either way is that no member reads a row written beside
//! it. Rows are landed in **desk order**, and within a desk in the order the
//! library authorized the round, whatever order the calls return — so the
//! schedule is deterministic: the same seed and the same `--jobs` produce the
//! same transcript.
//!
//! [round]: https://github.com/tinyhumansai/tinyhivemind/blob/main/docs/adr/0014-a-round-authorizes-concurrent-turns.md

use std::time::Instant;

use tinyhivemind_hive::{EpisodePolicy, EpisodeState, HiveStep, step};

use super::board::{
    Board, PlannedAnswer, PlannedTurn, SpokenAnswer, SpokenTurn, fill_answer, fill_turn,
};
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
        // Published before anything is asked, and for the same reason the ask
        // goes out before the desk has backed anything: a reading that lands
        // after the desk has reached quorum is information it has already
        // voted past. Bounded by the digest count, so this cannot keep the
        // loop alive on its own.
        if board.publish_digest(members, desk)? {
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
            HiveStep::Speak { turns, next_state } => {
                // A desk takes its whole round before the scheduler moves on,
                // so one desk's round interleaves with another desk's round
                // rather than with its turns. Each turn is landed before the
                // next is planned, which is what a `Visibility::Full` round
                // needs and what a blind one is indifferent to — `project_for`
                // withholds the peer rows either way.
                for turn in &turns {
                    let planned = board.plan_turn(members, desk, turn)?;
                    let said = {
                        let seats = &mut members[desk];
                        fill_turn(seats, &planned)?
                    };
                    board.land_turn(members, &said)?;
                }
                states[desk] = *next_state;
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
    // anybody speaks. Nothing here calls a model on a live desk — a pending
    // answer is *planned* rather than answered, and `publish` and `ask` are
    // both `None` for every live seat by default. Answering is a model call
    // and belongs in phase three with the turns.
    let mut answers: Vec<PlannedAnswer> = Vec::new();
    for (desk, outcome) in finished.iter().enumerate() {
        if outcome.is_some() {
            board.strand(desk);
            continue;
        }
        if let Some(incoming) = board.pop_pending(desk) {
            answers.push(board.plan_answer(members, desk, &incoming)?);
            continue;
        }
        if board.publish_digest(members, desk)? {
            progressed = true;
            continue;
        }
        if board.ask_off_floor(members, desk)? {
            progressed = true;
        }
    }
    // A desk that owes an answer does not also take a turn this pass, so the
    // two never contend for the same seats.
    let mut answering: Vec<bool> = vec![false; count];
    for planned in &answers {
        answering[planned.desk] = true;
    }

    // Phase two, sequential and cheap: ask the library who speaks next on each
    // desk, and retire the desks that are done.
    let mut planned: Vec<PlannedTurn> = Vec::new();
    let mut committed: Vec<Option<EpisodeState>> = vec![None; count];
    for desk in 0..count {
        if finished[desk].is_some() || answering[desk] || !board.pending_empty(desk) {
            continue;
        }
        match decide(board, states, policy, desk)? {
            HiveStep::Speak { turns, next_state } => {
                // Every turn of the round is planned against the journal as it
                // stands *before* the round, which is what makes the round
                // concurrent. That is the library's own reading of a round —
                // members writing simultaneously cannot read each other — and
                // it is why a wide round is a blind round.
                for turn in &turns {
                    planned.push(board.plan_turn(members, desk, turn)?);
                }
                committed[desk] = Some(*next_state);
            }
            ended => {
                retire(channels, finished, desk, ended);
                progressed = true;
            }
        }
    }

    // Phase three, concurrent: every turn of every desk's round is filled at
    // the same time. The width is now two-dimensional — `desks x round_width`
    // model calls in flight — and `--jobs` is what bounds it.
    // A desk either answers a referral or takes a round, never both, so one
    // work item per desk covers the whole federation's model calls.
    let spoken: Vec<Spoken> = {
        // Seats are borrowed per desk, so the work is grouped by desk and each
        // group fills its own round in order. Grouping rather than flattening
        // is not a nicety: two turns of one round need the same `&mut` seats.
        let mut work: Vec<(Job<'_>, &mut Vec<&mut dyn SwarmMember>)> = members
            .iter_mut()
            .enumerate()
            .filter_map(|(desk, seats)| {
                if let Some(answer) = answers.iter().find(|plan| plan.desk == desk) {
                    return Some((Job::Answer(answer), seats));
                }
                let round: Vec<&PlannedTurn> =
                    planned.iter().filter(|plan| plan.desk == desk).collect();
                if round.is_empty() {
                    return None;
                }
                Some((Job::Round(round), seats))
            })
            .collect();
        parallel::map_mut_in_order(&mut work, jobs, |(job, seats)| match job {
            Job::Answer(plan) => {
                fill_answer(seats, plan).map(|said| Spoken::Answer(Box::new(said)))
            }
            Job::Round(round) => round
                .iter()
                .map(|plan| fill_turn(seats, plan))
                .collect::<Result<Vec<SpokenTurn>, String>>()
                .map(Spoken::Round),
        })?
    };

    // Phase four, sequential and in desk order, each desk's round in the order
    // the library authorized it. The order is load-bearing rather than tidy: a
    // row consumes a sequence number, and the quorum window and salience decay
    // read raw sequence distance under `Basis::Sequence`, so landing in
    // completion order would let one seed decide two different things.
    for job in &spoken {
        match job {
            Spoken::Answer(answer) => board.land_answer(members, answer)?,
            Spoken::Round(round) => {
                for said in round {
                    board.land_turn(members, said)?;
                }
            }
        }
        progressed = true;
    }
    for (desk, next_state) in committed.into_iter().enumerate() {
        if let Some(next_state) = next_state {
            states[desk] = next_state;
        }
    }

    Ok(progressed)
}

/// One desk's model calls for this pass: an answer it owes, or the round it
/// was authorized.
///
/// A desk with a referral waiting does not also take a turn, so this is an
/// either/or rather than a pair — which is what lets the whole federation's
/// calls go through one concurrent stage.
enum Job<'a> {
    /// A question from another channel, waiting on this desk.
    Answer(&'a PlannedAnswer),
    /// The turns this desk's episode authorized.
    Round(Vec<&'a PlannedTurn>),
}

/// What one desk's job produced.
enum Spoken {
    /// The answer, and the referral it routes back along.
    ///
    /// Boxed because a `SpokenAnswer` carries the whole `Referral` it replies
    /// to and a `Round` carries a `Vec`, so the unboxed variants differ in
    /// size by enough that every round would pay for the answer's payload.
    Answer(Box<SpokenAnswer>),
    /// The round, in the order the library authorized it.
    Round(Vec<SpokenTurn>),
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
