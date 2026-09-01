//! Recency, importance and relevance, folded into one comparable score.
//!
//! The shape is the memory-stream retrieval score: a weighted sum of an
//! exponentially decaying recency term, a standing importance term, and a
//! caller-supplied relevance term. It is the whole attention and decay layer
//! in one function.
//!
//! Decay is not optional. Without it the participant who spoke first keeps the
//! floor forever, which is the failure mode ant trails avoid only because
//! pheromone evaporates.

#[cfg(test)]
mod test;

mod types;

pub use types::{Salience, SalienceWeights};

use crate::{
    error::{Error, Result},
    trace::{Trace, TraceKind},
};
use tinyhivemind::Sequence;

/// Fixed-point scale: scores and factors are thousandths.
const SCALE: i64 = 1_000;

/// Score one trace's pull on the room's attention.
///
/// `relevance` is a caller-supplied topical match in `0..=100`; a caller with
/// no notion of relevance passes a constant and gets recency and importance
/// alone. Values above 100 saturate.
///
/// # Errors
///
/// Returns [`Error::ZeroHalfLife`] when the weights would make recency
/// undefined.
pub fn salience(
    trace: &Trace,
    at: Sequence,
    weights: &SalienceWeights,
    relevance: u8,
) -> Result<Salience> {
    Ok(with_relevance(
        standing(trace, at, weights)?,
        weights,
        relevance,
    ))
}

/// The part of a trace's salience that does not depend on who is reading it.
///
/// Recency and importance are properties of the trace alone, so the attention
/// market computes them once per trace rather than once per member per trace.
///
/// # Errors
///
/// Returns [`Error::ZeroHalfLife`] when the weights would make recency
/// undefined.
pub(crate) fn standing(trace: &Trace, at: Sequence, weights: &SalienceWeights) -> Result<i64> {
    if weights.half_life == 0 {
        return Err(Error::ZeroHalfLife);
    }
    let distance = at.0.saturating_sub(trace.sequence.0);
    Ok(
        i64::from(weights.recency) * decay(distance, weights.half_life)
            + i64::from(weights.importance) * importance(trace.kind),
    )
}

/// Add one reader's topical relevance to a trace's standing salience.
pub(crate) fn with_relevance(standing: i64, weights: &SalienceWeights, relevance: u8) -> Salience {
    let relevance = i64::from(relevance.min(100)) * SCALE / 100;
    Salience((standing + i64::from(weights.relevance) * relevance) / 10)
}

/// Return the standing importance of a trace kind, in thousandths.
///
/// A proposal and a commitment move the room most; a question moves it least.
/// A refutation outranks an objection because it is directed at a hypothesis
/// rather than at a person, and is outranked by a proposal because putting a
/// new option on the floor moves the room further than removing one.
/// These are ordinal rather than measured, and are deliberately coarse.
#[must_use]
pub fn importance(kind: TraceKind) -> i64 {
    match kind {
        TraceKind::Commit => 1_000,
        TraceKind::Propose => 900,
        TraceKind::Refute => 850,
        TraceKind::Object => 800,
        TraceKind::Evidence => 600,
        TraceKind::Support => 500,
        TraceKind::Question => 300,
    }
}

/// Halve `SCALE` once per `half_life` of sequence distance, in thousandths.
///
/// Integer-only: the whole halvings are a shift, and the remainder is
/// interpolated linearly within the final half-life. Distances beyond ~63
/// half-lives saturate at zero rather than overflowing the shift.
fn decay(distance: u64, half_life: u32) -> i64 {
    let half_life = u64::from(half_life);
    let whole = distance / half_life;
    if whole >= 63 {
        return 0;
    }
    let remainder = distance % half_life;
    let high = SCALE >> whole;
    let low = high / 2;
    // Linear interpolation between the two adjacent halvings.
    let span = high - low;
    let Ok(remainder) = i64::try_from(remainder) else {
        return low;
    };
    let Ok(half_life) = i64::try_from(half_life) else {
        return low;
    };
    high - span * remainder / half_life
}
