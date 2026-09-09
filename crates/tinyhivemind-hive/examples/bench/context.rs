//! What a member can still *read* of what it has been told.
//!
//! Every other part of this harness models information as free once it
//! arrives: [`SimAgent::import`](super::sim::SimAgent::import) folds a peer's
//! reading into a running sum, and it stays there, at full strength, for the
//! rest of the episode. That is the right model for asking whether a protocol
//! *moves* information. It is the wrong model for asking what the moving
//! costs, and it makes one arm unbeatable by construction: `hive+pooled` hands
//! every member every peer's reading for free, and free is doing as much work
//! in that sentence as pooled is.
//!
//! A real participant is a language model with a window. Two findings from
//! [`../../../../docs/research/long-context.md`](../../../../docs/research/long-context.md)
//! say what that costs, and this module is those two findings and nothing
//! else:
//!
//! - **Position is not neutral.** Liu et al., *Lost in the Middle* (TACL 2024):
//!   retrieval is best at the beginning and end of the input and degrades
//!   markedly in the middle, in a U-shaped curve. A message that arrives in the
//!   middle of a larger window is *less* likely to be used than the same
//!   message at the edge of a smaller one.
//! - **Degradation is not a cliff at the limit.** Quality falls off well before
//!   a model runs out of window, and faster when the input is dense and
//!   distractor-rich. A window is a **budget**, not a capacity.
//!
//! # What this is, and is not
//!
//! It is a *model*, with the shape the literature reports and parameters this
//! harness sweeps. It is **not** a measurement of any real model's retrieval,
//! and no number it produces should be read as one. Its job is to answer a
//! question the rest of the benchmark cannot ask at all — *who is still right
//! when the window is tight* — and to answer it in a way that can be argued
//! with, by making both parameters explicit and sweepable rather than baked in.
//!
//! A finding that only holds at one setting of [`ContextBudget::rot`] is a
//! finding about this file. The sweep in `--context-sweep` exists so that can
//! be seen rather than assumed: what is worth reporting is an ordering that
//! survives the whole plausible range.
//!
//! # Off by default
//!
//! [`ContextBudget::UNBOUNDED`] has `capacity: 0`, which every read path
//! short-circuits on, so a run that does not ask for a budget is bit-identical
//! to one built before this module existed. That is the same discipline
//! `set_aside_cap` follows: a mechanism nobody switched on changes no number.

use tinyhivemind_hive::TopicId;

/// One thing a member has been told, occupying one row of its window.
///
/// The unit is a row rather than a token because that is the unit this
/// harness's transcript is already counted in — `SESSION_WINDOW`, `PIN_SCAN`
/// and every other bound in the workspace count rows inspected. A row is a
/// coarse proxy for a token cost, and a deliberately conservative one: it
/// prices a four-message aside and a four-message argument the same, when the
/// aside is in fact the shorter of the two.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ContextEntry {
    /// The option this row says something about.
    pub(crate) topic: TopicId,
    /// What the row carries.
    pub(crate) kind: EntryKind,
}

/// What one row is worth to the member holding it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EntryKind {
    /// A peer's numeric reading of the option, to be averaged in.
    Reading(i32),
    /// A fact that rules the option out.
    Fact,
    /// An exchange this member was not part of: it can see that the exchange
    /// happened and who was in it, and cannot read what was said.
    ///
    /// **This is the honest cost of auditability**, and it is charged here
    /// rather than waved away: a stub occupies a row and carries nothing. An
    /// aside is only cheap for a non-member because the projection collapses a
    /// whole exchange into one of these, not because it is free.
    Stub,
}

/// What happens to a row the window has no space for.
///
/// Two different events with two different costs, and a benchmark that
/// modelled only the first would be measuring a soloist at its worst rather
/// than at its best.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Compaction {
    /// It is **dropped**. What is gone is gone.
    ///
    /// The default, and what every arm ran under before `--stages` existed.
    /// It is the pessimistic-but-documented reading of what a sliding window
    /// does to a transcript nobody curated.
    #[default]
    Evict,
    /// It is **summarised**: it gives up its position in the window and still
    /// contributes, at [`FOLD_FIDELITY`].
    ///
    /// A deliberately *generous* model of compaction. The point of the arm
    /// that uses it is to make a single agent as strong as it can honestly be
    /// made, so that a room beating it is beating something real. A summary
    /// that loses two thirds of a fact's force but never loses the fact is
    /// better than any compaction this workspace has actually measured.
    Fold,
}

/// What a summarised row is still worth, against the 1.0 of a row in the
/// window.
///
/// Not measured, and it is a parameter rather than a constant for that reason:
/// `--fidelity` sweeps it, and a finding that only holds at one setting is a
/// finding about this file. See [`ContextBudget::fidelity`].
pub(crate) const FOLD_FIDELITY: f64 = 0.35;

/// A member's window: how much it can hold, and how badly the middle of it
/// degrades.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ContextBudget {
    /// Rows the window holds. `0` disables the whole model.
    pub(crate) capacity: usize,
    /// What becomes of a row past [`Self::capacity`].
    pub(crate) compaction: Compaction,
    /// What a summarised row is worth under [`Compaction::Fold`].
    ///
    /// Ignored under [`Compaction::Evict`], where an overflowing row is worth
    /// nothing whatever this says.
    pub(crate) fidelity: f64,
    /// How strongly the middle of the window is discounted, in `0.0..=1.0`.
    ///
    /// `0.0` is a flat window: every retained row is worth full value, and the
    /// only cost of a crowded window is eviction. `1.0` is the strongest
    /// U-curve this model expresses — a row at the exact centre is worth
    /// nothing, a row at either edge is worth everything.
    ///
    /// Neither extreme is a claim about a real model. They bracket it.
    pub(crate) rot: f64,
}

impl ContextBudget {
    /// No window at all: what every arm ran under before this module.
    pub(crate) const UNBOUNDED: Self = Self {
        capacity: 0,
        compaction: Compaction::Evict,
        fidelity: FOLD_FIDELITY,
        rot: 0.0,
    };

    /// What a row at `index` is worth, given the rows the window retained.
    ///
    /// One function for both halves of the model, because the two are one
    /// question — *how much of this row can still be read* — and answering it
    /// in two places is how a caller ends up charging an evicted row a
    /// positional discount it was never in a position to earn.
    ///
    /// A retained row is worth its position's weight. A row that did not
    /// survive is worth nothing under [`Compaction::Evict`] and
    /// [`Self::fidelity`] under [`Compaction::Fold`] — a summary keeps no
    /// position, so it takes no positional discount either.
    pub(crate) fn worth(&self, index: usize, kept: &[usize]) -> f64 {
        match kept.iter().position(|held| *held == index) {
            Some(position) => self.weight(position, kept.len()),
            None => match self.compaction {
                Compaction::Evict => 0.0,
                Compaction::Fold => self.fidelity,
            },
        }
    }

    /// Whether this budget models anything.
    pub(crate) const fn is_unbounded(&self) -> bool {
        self.capacity == 0
    }

    /// The weight one row carries, given its position and how full the window
    /// is.
    ///
    /// `at` is the row's index in arrival order and `held` the number of rows
    /// retained. The curve is symmetric about the centre: distance from the
    /// middle, normalised to `0.0..=1.0`, scaled by [`Self::rot`].
    ///
    /// A single row is all edge and never discounted — with nothing to be in
    /// the middle *of*, the U-curve has no middle.
    #[expect(
        clippy::cast_precision_loss,
        reason = "a window holding more rows than an f64 can index is not a window"
    )]
    pub(crate) fn weight(&self, at: usize, held: usize) -> f64 {
        if self.is_unbounded() || held <= 1 || self.rot <= 0.0 {
            return 1.0;
        }
        // Position in 0..=1, then distance from the centre in 0..=1: 1.0 at
        // either edge, 0.0 at the exact middle.
        let position = at as f64 / (held - 1) as f64;
        let from_centre = (2.0 * position - 1.0).abs();
        // `rot` interpolates between a flat window and the full U.
        1.0 - self.rot * (1.0 - from_centre)
    }

    /// The rows that survive compaction, as indices into `entries`.
    ///
    /// Over capacity, the **middle** is what goes. That is the pessimistic-but
    /// -documented reading of what compaction does to a transcript: the head
    /// carries the instructions and the tail carries what just happened, so a
    /// summariser under pressure spends its budget on the ends and the middle
    /// is what it drops or paraphrases away. It is also the same shape as the
    /// retrieval curve, which is the point — the middle is both the least
    /// likely to be read and the first to be discarded.
    pub(crate) fn retained(&self, count: usize) -> Vec<usize> {
        if self.is_unbounded() || count <= self.capacity {
            return (0..count).collect();
        }
        // Keep the ends, drop from the centre outwards. The head gets the odd
        // row when the capacity is odd: an instruction is worth more than a
        // recency.
        let keep = self.capacity;
        let head = keep.div_ceil(2);
        let tail = keep - head;
        (0..head).chain(count - tail..count).collect()
    }
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self::UNBOUNDED
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// Compare two weights.
    ///
    /// The values here are exact in principle — a `rot` of zero discounts
    /// nothing, an edge is worth one, the exact centre is worth zero — but they
    /// arrive through floating-point arithmetic, and a test that asserts on the
    /// bit pattern of a computed `f64` is asserting on the arithmetic rather
    /// than on the rule.
    fn close(left: f64, right: f64) -> bool {
        (left - right).abs() < 1e-9
    }

    #[test]
    fn an_unbounded_budget_changes_nothing() {
        let budget = ContextBudget::UNBOUNDED;
        assert!(budget.is_unbounded());
        assert_eq!(budget.retained(50), (0..50).collect::<Vec<_>>());
        for at in 0..50 {
            assert!(close(budget.weight(at, 50), 1.0));
        }
    }

    #[test]
    fn a_flat_window_evicts_but_does_not_discount() {
        let budget = ContextBudget {
            capacity: 4,
            rot: 0.0,
        };
        assert_eq!(budget.retained(4), vec![0, 1, 2, 3]);
        // Six rows into four: the middle two go.
        assert_eq!(budget.retained(6), vec![0, 1, 4, 5]);
        for at in 0..4 {
            assert!(
                close(budget.weight(at, 4), 1.0),
                "a flat window discounts nothing"
            );
        }
    }

    /// The U: edges keep their value, the centre loses it.
    #[test]
    fn the_middle_of_a_window_is_worth_less_than_its_edges() {
        let budget = ContextBudget {
            capacity: 16,
            rot: 1.0,
        };
        let held = 5;
        assert!(
            close(budget.weight(0, held), 1.0),
            "the first row is all edge"
        );
        assert!(close(budget.weight(4, held), 1.0), "so is the last");
        assert!(
            close(budget.weight(2, held), 0.0),
            "the exact centre is worth nothing"
        );
        assert!(
            budget.weight(1, held) > budget.weight(2, held),
            "and the curve is monotone towards the centre"
        );
        assert!((budget.weight(1, held) - budget.weight(3, held)).abs() < 1e-9);
    }

    /// `rot` interpolates rather than switching: half the rot, half the loss.
    #[test]
    fn rot_scales_the_discount_it_applies() {
        let strong = ContextBudget {
            capacity: 16,
            rot: 1.0,
        };
        let half = ContextBudget {
            capacity: 16,
            rot: 0.5,
        };
        assert!(close(strong.weight(2, 5), 0.0));
        assert!(close(half.weight(2, 5), 0.5));
    }

    /// A lone row has no middle to fall into.
    #[test]
    fn one_row_is_never_discounted() {
        let budget = ContextBudget {
            capacity: 8,
            rot: 1.0,
        };
        assert!(close(budget.weight(0, 1), 1.0));
    }

    /// The property the whole file exists for, stated correctly.
    ///
    /// A first cut of this test asserted that crowding a window devalues
    /// everything already in it, and **that is false** under a U-curve: an
    /// entry keeps its index while `held` grows, so its normalised position
    /// moves *towards the head*, which is a privileged edge. The entry gets
    /// better, not worse. The code was right and the test was wrong.
    ///
    /// What crowding actually costs is this: a window holds a fixed number of
    /// good positions, and everything past them lands in the middle. With one
    /// row, that row is all edge. With many, most of them are not — so the
    /// *middle* of a full window is where the majority of its content sits,
    /// and that is the content least likely to be read.
    #[test]
    fn a_full_window_puts_most_of_its_content_in_the_cheap_middle() {
        let budget = ContextBudget {
            capacity: 64,
            rot: 1.0,
        };
        // Rows worth less than half their face value. Counted rather than
        // divided: the claim is about a share, and comparing two shares as a
        // cross-multiplication of counts keeps the assertion in integers.
        let buried = |held: usize| {
            (0..held)
                .filter(|at| budget.weight(*at, held) < 0.5)
                .count()
        };
        assert_eq!(buried(1), 0, "a lone row is all edge");
        assert!(
            buried(21) * 10 > 21 * 4,
            "most of a full window sits in its own middle: {} of 21",
            buried(21)
        );
        // And the share does not shrink as it fills further.
        assert!(buried(41) * 21 >= buried(21) * 41);
    }

    /// The mechanism that actually drove `hive+pooled` down in the sweep:
    /// once a window is over capacity, the middle is not merely cheap, it is
    /// **gone**. An entry that would have been read at half value is read at
    /// none.
    #[test]
    fn over_capacity_the_middle_is_dropped_not_discounted() {
        let budget = ContextBudget {
            capacity: 6,
            rot: 0.0,
        };
        let kept = budget.retained(17);
        assert_eq!(kept.len(), 6, "the window holds what it holds");
        for middle in 4..13 {
            assert!(
                !kept.contains(&middle),
                "row {middle} is in the middle of seventeen and should be gone"
            );
        }
    }

    /// Eviction keeps both ends, so the newest row always survives.
    #[test]
    fn compaction_keeps_the_ends_and_drops_the_centre() {
        let budget = ContextBudget {
            capacity: 5,
            rot: 0.0,
        };
        let kept = budget.retained(11);
        assert_eq!(kept.len(), 5);
        assert!(kept.contains(&0), "the head survives");
        assert!(kept.contains(&10), "and so does the newest row");
        assert!(!kept.contains(&5), "the exact centre does not");
    }
}

#[cfg(test)]
mod fold_test {
    use super::*;

    /// Eviction is the default, and it is what every arm ran under before
    /// folding existed.
    #[test]
    fn an_evicted_row_is_worth_nothing_and_a_folded_one_is_not() {
        let evicting = ContextBudget {
            capacity: 4,
            rot: 0.0,
            ..ContextBudget::UNBOUNDED
        };
        assert_eq!(evicting.compaction, Compaction::Evict);
        let kept = evicting.retained(10);
        // Row 5 is in the middle of ten and did not survive four.
        assert!(!kept.contains(&5));
        assert!((evicting.worth(5, &kept) - 0.0).abs() < 1e-9);

        let folding = ContextBudget {
            compaction: Compaction::Fold,
            ..evicting
        };
        assert!((folding.worth(5, &kept) - FOLD_FIDELITY).abs() < 1e-9);
    }

    /// A row that *did* survive is worth its position either way: folding
    /// changes what happens to the overflow, never what happens to the window.
    #[test]
    fn folding_does_not_change_what_a_retained_row_is_worth() {
        let evicting = ContextBudget {
            capacity: 8,
            rot: 1.0,
            ..ContextBudget::UNBOUNDED
        };
        let folding = ContextBudget {
            compaction: Compaction::Fold,
            ..evicting
        };
        let kept = evicting.retained(20);
        for index in &kept {
            assert!((evicting.worth(*index, &kept) - folding.worth(*index, &kept)).abs() < 1e-9);
        }
    }

    /// A summary keeps no position, so it takes no positional discount — which
    /// is the whole reason a folded row can be worth more than a retained one
    /// that landed in the middle of a crowded window.
    #[test]
    fn a_summary_can_beat_the_middle_of_a_full_window() {
        let folding = ContextBudget {
            capacity: 9,
            compaction: Compaction::Fold,
            fidelity: FOLD_FIDELITY,
            rot: 1.0,
        };
        let kept = folding.retained(30);
        let centre = kept.len() / 2;
        let buried = kept[centre];
        assert!(
            folding.worth(buried, &kept) < folding.worth(15, &kept),
            "a row summarised out of the window beats one buried in its middle"
        );
    }

    /// An unbounded window retains everything, so nothing is ever summarised
    /// and the mode cannot change a number.
    #[test]
    fn an_unbounded_window_folds_nothing() {
        let folding = ContextBudget {
            compaction: Compaction::Fold,
            ..ContextBudget::UNBOUNDED
        };
        let kept = folding.retained(50);
        for index in 0..50 {
            assert!((folding.worth(index, &kept) - 1.0).abs() < 1e-9);
        }
    }
}
