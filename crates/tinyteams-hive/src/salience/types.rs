//! Stable salience inputs and scores.

use serde::{Deserialize, Serialize};

/// Fixed-point weights for the three salience terms.
///
/// Weights are tenths, so `30` means `3.0`. Fixed point rather than floating
/// point because every payload type in this workspace derives [`Eq`] and pins
/// an exact wire form, and because a fold that has to be reproducible cannot
/// depend on floating-point association.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct SalienceWeights {
    /// Weight on exponential recency decay, in tenths.
    pub recency: u16,
    /// Weight on the trace kind's standing importance, in tenths.
    pub importance: u16,
    /// Weight on caller-supplied topical relevance, in tenths.
    pub relevance: u16,
    /// Sequence distance at which the recency term halves.
    pub half_life: u32,
}

impl SalienceWeights {
    /// The weights the original memory-stream implementation actually shipped.
    ///
    /// Its paper and its code disagree; these are the code's — `0.5`, `3.0`,
    /// `2.0` — and decay is applied by rank in sequence rather than by elapsed
    /// time, which is also what that implementation did and what a transcript
    /// with no clock can support.
    pub const DEFAULT: Self = Self {
        recency: 5,
        importance: 30,
        relevance: 20,
        half_life: 20,
    };
}

impl Default for SalienceWeights {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// A fixed-point salience score in thousandths.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Salience(pub i64);
