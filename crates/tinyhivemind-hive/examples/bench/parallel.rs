//! Spreading the per-room loops across cores, without changing a number.
//!
//! Every arm decides the same rooms, and a room is generated from
//! `mix(seed, index)` alone (see [`crate::rng::mix`]) — it shares no state with
//! any other room and reads nothing a previous room wrote. The per-room loops
//! are therefore embarrassingly parallel, and at the sizes this harness is now
//! asked about they are the whole runtime: a thousand-member room runs about
//! three thousand turns, each folding a transcript that is already thousands of
//! rows long, and seven arms decide every one of them.
//!
//! # Why the results cannot move
//!
//! Two rules, and both are enforced here rather than left to a caller:
//!
//! - **Work is split, never shared.** Each room is handed to exactly one
//!   thread, and the closure is given `&T` — so nothing a room does can be seen
//!   by another room, in either order.
//! - **Results are returned in input order, never completion order.** Threads
//!   finish in whatever order the scheduler picks; this function hands back a
//!   `Vec<R>` indexed exactly as `items` was. That matters more than it looks:
//!   [`crate::metrics::Aggregate::correct_flags`] is positional and
//!   `paired_bootstrap` resamples two arms index-for-index because they decided
//!   the *same* rooms, so folding in completion order would silently corrupt
//!   every confidence interval in the second table while leaving the accuracy
//!   column looking fine.
//!
//! The consequence worth stating: `--jobs` is a wall-clock knob and nothing
//! else. The same `--seed` must print the same bytes at `--jobs 1` and
//! `--jobs 64`, and `crate::stats_check` is not the thing that checks it — the
//! test at the bottom of this file is.
//!
//! # Why scoped threads rather than a pool crate
//!
//! `tinyhivemind-hive` is a pure crate: `.github/scripts/assert-pure.sh`
//! forbids an async runtime or a transport anywhere in its tree, and the crate
//! carries exactly one dev-dependency (`serde_json`). `std::thread::scope`
//! needs neither, borrows `items` without cloning it, and joins every thread
//! before returning — which is what lets the closure take a plain reference.

/// How many threads to use when the operator did not say.
///
/// One per core, and no fallback cleverness: if the platform cannot report its
/// parallelism the honest answer is to stay sequential rather than to guess a
/// number that might be four times the machine.
pub(crate) fn default_jobs() -> usize {
    std::thread::available_parallelism().map_or(1, std::num::NonZero::get)
}

/// Apply `work` to every item, in parallel, returning results in input order.
///
/// `jobs` is clamped to at least one and to at most the number of items, so a
/// caller may pass a raw `--jobs` value without reasoning about a short slice.
/// At one job nothing is spawned at all, which keeps the sequential path free
/// of thread overhead and gives `--jobs 1` a meaning precise enough to compare
/// against.
///
/// # Errors
///
/// Returns the first error in *input* order, not the first one raised. Two
/// rooms failing in different threads would otherwise report whichever thread
/// happened to lose the race, and a benchmark that reports a different failure
/// on every run is a benchmark nobody debugs.
pub(crate) fn map_in_order<T, R, F>(items: &[T], jobs: usize, work: F) -> Result<Vec<R>, String>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> Result<R, String> + Sync,
{
    let jobs = jobs.max(1).min(items.len().max(1));
    if jobs == 1 || items.len() < 2 {
        return items.iter().map(&work).collect();
    }

    // Contiguous chunks rather than a work-stealing queue. Rooms at one swept
    // size cost about the same as each other, so the simplest split is also a
    // balanced one, and a chunk keeps its results already in order.
    let chunk = items.len().div_ceil(jobs);
    let mut chunks: Vec<Result<Vec<R>, String>> = Vec::new();
    std::thread::scope(|scope| {
        let handles: Vec<_> = items
            .chunks(chunk)
            .map(|slice| {
                scope.spawn(|| slice.iter().map(&work).collect::<Result<Vec<R>, String>>())
            })
            .collect();
        for handle in handles {
            // A panicking closure is a bug in an arm rather than a benchmark
            // outcome, so it is reported as an error rather than resumed into
            // the parent, where it would take down every other chunk's results
            // with it and say nothing about which room caused it.
            chunks.push(
                handle
                    .join()
                    .unwrap_or_else(|_| Err("a benchmark thread panicked".to_owned())),
            );
        }
    });

    let mut out = Vec::with_capacity(items.len());
    for chunk in chunks {
        out.extend(chunk?);
    }
    Ok(out)
}

/// Apply `work` to every item **mutably**, in parallel, in input order.
///
/// The counterpart to [`map_in_order`] for work that needs exclusive access to
/// the thing it is working on. What it exists for is a live federation: each
/// desk holds its own seats, its own journal and its own episode state, and
/// each authorizes exactly one speaker — so a pass over `N` desks is `N` model
/// calls that could all be in flight at once, and today are not.
///
/// The ordering rule is the same one and matters for the same reason: results
/// come back indexed as `items` was, so the caller appends rows in desk order
/// however the calls happen to return. That is not cosmetic. A private or desk
/// row consumes a sequence number, and `QuorumPolicy::window` and salience
/// decay read a *raw* sequence distance — so appending in completion order
/// would let the same federation, on the same seed, decide differently from
/// one run to the next.
///
/// # Errors
///
/// Returns the first error in input order, on the same contract as
/// [`map_in_order`].
pub(crate) fn map_mut_in_order<T, R, F>(
    items: &mut [T],
    jobs: usize,
    work: F,
) -> Result<Vec<R>, String>
where
    T: Send,
    R: Send,
    F: Fn(&mut T) -> Result<R, String> + Sync,
{
    let jobs = jobs.max(1).min(items.len().max(1));
    if jobs == 1 || items.len() < 2 {
        return items.iter_mut().map(&work).collect();
    }

    let chunk = items.len().div_ceil(jobs);
    let mut chunks: Vec<Result<Vec<R>, String>> = Vec::new();
    std::thread::scope(|scope| {
        let handles: Vec<_> = items
            .chunks_mut(chunk)
            .map(|slice| {
                scope.spawn(|| slice.iter_mut().map(&work).collect::<Result<Vec<R>, String>>())
            })
            .collect();
        for handle in handles {
            chunks.push(
                handle
                    .join()
                    .unwrap_or_else(|_| Err("a benchmark thread panicked".to_owned())),
            );
        }
    });

    let mut out = Vec::with_capacity(items.len());
    for chunk in chunks {
        out.extend(chunk?);
    }
    Ok(out)
}

#[cfg(test)]
mod test;
