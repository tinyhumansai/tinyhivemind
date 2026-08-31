//! A deterministic, dependency-free pseudo-random generator.
//!
//! The benchmark must be reproducible: the same seed has to produce the same
//! rooms, the same private evaluations, and the same transcripts, so a change
//! in a reported number is always a change in the library rather than in the
//! weather. `SplitMix64` is used because it is four lines long, passes the
//! usual statistical batteries, and adds no dependency to a workspace that
//! guards its dependency graph.

/// A `SplitMix64` generator.
#[derive(Clone, Debug)]
pub(crate) struct Rng {
    state: u64,
}

impl Rng {
    /// Seed a generator.
    pub(crate) const fn seeded(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(0x9E37_79B9_7F4A_7C15),
        }
    }

    /// Draw the next 64 bits.
    pub(crate) fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Draw a value in `0..bound`, or `0` when `bound` is zero.
    pub(crate) fn below(&mut self, bound: u32) -> u32 {
        if bound == 0 {
            return 0;
        }
        let drawn = self.next_u64() % u64::from(bound);
        u32::try_from(drawn).unwrap_or(0)
    }

    /// Draw a value in `-spread..=spread`.
    pub(crate) fn centered(&mut self, spread: u32) -> i32 {
        let width = spread.saturating_mul(2).saturating_add(1);
        let drawn = i64::from(self.below(width)) - i64::from(spread);
        i32::try_from(drawn).unwrap_or(0)
    }

    /// Return whether a per-mille chance fired.
    pub(crate) fn chance(&mut self, per_mille: u32) -> bool {
        self.below(1_000) < per_mille
    }
}

/// Mix two values into one stable seed.
///
/// Used to derive a per-agent generator from the room seed, so an agent's
/// private evaluations do not shift when an unrelated agent is added.
pub(crate) fn mix(left: u64, right: u64) -> u64 {
    let mut rng = Rng::seeded(left ^ right.wrapping_mul(0xD6E8_FEB8_6659_FD93));
    rng.next_u64()
}
