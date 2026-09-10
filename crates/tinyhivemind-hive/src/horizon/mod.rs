//! Where a fold is measured to, and how distance back from it is counted.
//!
//! Every fold in this crate is *local*: quorum counts support within a window,
//! salience decays a trace by how far back it is, and the directory does both.
//! All three ask the same question — how far is this trace from where the room
//! is now — and until this type existed all three answered it by subtracting
//! raw [`Sequence`] values.
//!
//! # Why a raw sequence is the wrong ruler
//!
//! A sequence numbers a row in the host's transcript, not a row in the
//! episode. The episode folds a *subset* of that transcript: rows above the
//! watermark, addressed to the desk, authored by a current member. Everything
//! else — the conversation that preceded the episode, a private aside, a row
//! from a retired agent, an off-floor question put to another channel — still
//! consumes a sequence number, and under raw arithmetic still counts against
//! the window and against the decay.
//!
//! So a desk that is busy in ways the episode is not reading has a quietly
//! shorter window than an idle one, on the same policy, deciding the same
//! question. Thirty sequences of a quiet desk is thirty rows of deliberation;
//! thirty sequences of a desk also running asides may be ten. Nothing reports
//! it, and the room simply decides less well.
//!
//! [`Basis::Live`] measures in the rows the fold actually reads. On a dense
//! journal the episode owns end to end the two are identical, which is why the
//! benchmark's recorded numbers are unchanged by it; they diverge exactly when
//! the transcript carries something the episode is not folding, which is the
//! case a real host is in and the harness is not.
//!
//! # Why it is not the default
//!
//! Two mechanisms in this crate have already been measured, lost, and been
//! left opt-in rather than quietly defaulted on. This one has not yet been
//! scored on an arm where it makes a difference, so it ships the same way:
//! [`Basis::Sequence`] is what [`EpisodePolicy::DEFAULT`] carries, and a host
//! says so to get the other.
//!
//! [`EpisodePolicy::DEFAULT`]: crate::episode::EpisodePolicy::DEFAULT

#[cfg(test)]
mod test;

use serde::{Deserialize, Serialize};

use tinyhivemind::Sequence;

/// How distance between two rows is counted.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    /// Subtract raw [`Sequence`] values.
    ///
    /// Counts every row the host wrote, including the ones the episode does
    /// not fold. The original behaviour, and what [`EpisodePolicy::DEFAULT`]
    /// carries.
    ///
    /// [`EpisodePolicy::DEFAULT`]: crate::episode::EpisodePolicy::DEFAULT
    #[default]
    Sequence,
    /// Count the rows the fold actually reads.
    ///
    /// A window of thirty means thirty folded rows *back from* the horizon —
    /// so the row at the horizon and the thirty before it, thirty-one in all,
    /// exactly as a window of thirty sequences admits the sequence at `at` and
    /// the thirty below it. What changes is the unit, not the arithmetic: the
    /// policy then means the same thing on a busy desk as on a quiet one.
    Live,
}

/// The sequence a fold is measured to, and the ruler it measures with.
///
/// Constructed from a bare [`Sequence`] for [`Basis::Sequence`], which is what
/// every caller that has not opted in gets:
///
/// ```
/// use tinyhivemind::Sequence;
/// use tinyhivemind_hive::horizon::Horizon;
///
/// let horizon = Horizon::from(Sequence(40));
/// assert_eq!(horizon.distance(Sequence(10)), 30);
/// ```
///
/// Or over the rows a fold reads, for [`Basis::Live`]:
///
/// ```
/// use tinyhivemind::Sequence;
/// use tinyhivemind_hive::horizon::Horizon;
///
/// // Four folded rows scattered across forty sequences of transcript.
/// let rows = [Sequence(10), Sequence(20), Sequence(30), Sequence(40)];
/// let horizon = Horizon::over(Sequence(40), &rows);
/// assert_eq!(horizon.distance(Sequence(10)), 3);
/// ```
#[derive(Clone, Copy, Debug)]
pub struct Horizon<'a> {
    /// The sequence the fold is measured to.
    at: Sequence,
    /// The folded rows, ascending, when measuring in rows rather than
    /// sequences.
    rows: Option<&'a [Sequence]>,
}

impl<'a> Horizon<'a> {
    /// Measure in raw sequence distance to `at`.
    #[must_use]
    pub const fn at(at: Sequence) -> Self {
        Self { at, rows: None }
    }

    /// Measure in `rows` — the rows the fold reads — up to `at`.
    ///
    /// `rows` must be ascending; it is the transcript order the caller already
    /// folded in. A sequence that is not one of them is measured to the
    /// position it would occupy, so the answer stays monotone rather than
    /// becoming a special case.
    #[must_use]
    pub const fn over(at: Sequence, rows: &'a [Sequence]) -> Self {
        Self {
            at,
            rows: Some(rows),
        }
    }

    /// The sequence being measured to.
    #[must_use]
    pub const fn sequence(self) -> Sequence {
        self.at
    }

    /// How far back `from` is, in whichever unit this horizon counts.
    ///
    /// Zero for anything at or after the horizon, so the answer is never
    /// negative and a caller never has to check.
    #[must_use]
    pub fn distance(self, from: Sequence) -> u64 {
        match self.rows {
            None => self.at.0.saturating_sub(from.0),
            Some(rows) => {
                let here = position(rows, self.at);
                let there = position(rows, from);
                here.saturating_sub(there)
            }
        }
    }

    /// Whether `from` is at or before the horizon and within `window` of it.
    ///
    /// `window` is a **distance back**, so the horizon's own row is always in
    /// and a window of `n` admits `n + 1` rows. That is what a window of
    /// sequences has always meant here; measuring in folded rows changes the
    /// unit and leaves the bound alone.
    ///
    /// The two halves are separate questions: a row *after* the horizon is out
    /// of scope however close it is, and a row before it is in scope only
    /// while the window reaches back that far.
    #[must_use]
    pub fn within(self, from: Sequence, window: u32) -> bool {
        from <= self.at && self.distance(from) <= u64::from(window)
    }
}

impl From<Sequence> for Horizon<'_> {
    fn from(at: Sequence) -> Self {
        Self::at(at)
    }
}

/// Where `sequence` sits among ascending `rows`.
///
/// The count of rows strictly before it, so a sequence that is present gets
/// its own index and one that is absent gets the index it would be inserted
/// at. That is what keeps [`Horizon::distance`] monotone in the sequence for
/// rows the fold never read.
fn position(rows: &[Sequence], sequence: Sequence) -> u64 {
    let index = rows.partition_point(|row| *row < sequence);
    u64::try_from(index).unwrap_or(u64::MAX)
}
