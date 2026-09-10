//! [`Totals`]: every arm's running totals over one sample of rooms, and the
//! bookkeeping that keeps a parallel fold of them consistent with a serial
//! one.
//!
//! Split out of [`super`] because the field list is the one place this
//! benchmark says, arm by arm, what each control isolates -- reading it
//! alongside `compare`'s driving loop would bury that under the loop's own
//! noise.

use crate::metrics::Aggregate;

/// Every arm's running totals over one sample of rooms.
#[derive(Default)]
pub(super) struct Totals {
    /// A deliberation at the crate's own default policy.
    pub(super) hive_default: Aggregate,
    /// The same at the tuned policy the sweep picked.
    pub(super) hive_tuned: Aggregate,
    /// The tuned policy with `refutation_cap` on.
    pub(super) hive_refuting: Aggregate,
    /// The same, plus `require_evidential`.
    pub(super) hive_evidential: Aggregate,
    /// The tuned policy with the folded directory on.
    pub(super) hive_knowing: Aggregate,
    /// The tuned policy with `!defer` bounded, and no directory.
    pub(super) hive_deferring: Aggregate,
    /// The tuned policy, with a member that cannot separate its two best
    /// options spending a turn asking one peer — privately.
    pub(super) hive_aside: Aggregate,
    /// The identical exchange, in the open. The control that isolates
    /// privacy from asking: same turns, same words, every member reads it.
    pub(super) hive_ask: Aggregate,
    /// The private exchange again, aimed at whoever the room has heard ground
    /// the option rather than at whoever spoke first.
    pub(super) hive_aside_informed: Aggregate,
    /// The aimed private exchange, carrying the fact that rules an option out
    /// rather than a reading of it to be averaged.
    pub(super) hive_aside_fact: Aggregate,
    /// The same exchange on the same turns, with the answer discarded.
    pub(super) hive_aside_mute: Aggregate,
    /// The aimed, fact-carrying exchange again, riding alongside each
    /// member's floor move rather than replacing one: one turn, two rows, and
    /// the room charged for the first only.
    pub(super) hive_aside_alongside: Aggregate,
    /// The free row spent continuously: a contact on every turn, carrying
    /// every reading its author holds. Bounded by `--aside-cap` peers.
    pub(super) hive_aside_exchange: Aggregate,
    /// The same continuous exchange, run **off the floor** in rounds between
    /// turns: nobody takes the floor for it, so its volume is set by the
    /// policy rather than by how many turns the room happens to take. Its
    /// price is the `calls/ep` column.
    pub(super) hive_exchange_rounds: Aggregate,
    /// `hive+share` with every answer discarded: the same rows riding
    /// alongside the same turns, transferring nothing. Alongside rows land
    /// unevenly — only on turns whose author wanted one — so unlike a round
    /// they do not shift every gap by a constant, and this is what says
    /// whether that unevenness moves the room on its own.
    pub(super) hive_aside_hush: Aggregate,
    /// The off-floor rounds again with every answer discarded: same rows,
    /// same sequence numbers consumed, nothing transferred. What it moves is
    /// what writing the rows does to salience decay, not what they said.
    pub(super) hive_exchange_quiet: Aggregate,
    /// The aimed, fact-carrying exchange again, held off the floor: the same
    /// bounded number of contacts, spending no turn the room could have
    /// deliberated with.
    pub(super) hive_aside_offfloor: Aggregate,
    /// The ceiling: every reading and every fact already in every member's
    /// hands before the episode opens, at no turn cost. Nothing a protocol
    /// could do beats it.
    pub(super) hive_pooled: Aggregate,
    /// The tuned policy run in **concurrent rounds** rather than one turn at a
    /// time. The arm ADR 0014 has to earn its place against: what should move
    /// is `rounds/ep`, and `correct %` says what the depth cost.
    pub(super) hive_wide: Aggregate,
    /// The same, widened **only while the room is blind**. The free half of
    /// concurrency on its own: a blind member cannot read a peer's row whether
    /// or not it runs concurrently, so this should score what `hive+` scores
    /// and wait fewer times.
    pub(super) hive_blind_wide: Aggregate,
    /// Both delegation mechanisms at once.
    pub(super) hive_both: Aggregate,
    /// The tuned policy in a room that puts every seat on the expensive
    /// tier. Only filled under `--cost-tiers`.
    pub(super) all_reasoning: Aggregate,
    /// The matched-budget independent poll.
    pub(super) vote: Aggregate,
    /// One responder off the real ladder, chosen without information.
    pub(super) ladder: Aggregate,
    /// The same ladder, given a directory the room earned.
    pub(super) ladder_directed: Aggregate,
}

impl Totals {
    /// An empty set of totals, every arm priced against `model`.
    ///
    /// The whole table reports at one point of the cost model, which is what
    /// lets the header describe every row under it and what
    /// [`Aggregate::merge`]'s debug assertion holds the parallel fold to.
    pub(super) fn priced_at(model: crate::cost::CostModel) -> Self {
        let mut totals = Self::default();
        for arm in totals.arms_mut() {
            arm.model = model;
        }
        // `ladder_directed` sits outside `arms_mut`'s array for the same
        // reason `merge` handles it explicitly (see below), but it is priced
        // like every other row: left out here, it would keep
        // `Aggregate::default()`'s model -- `CostModel::DEFAULT` -- instead
        // of the model a `--tokens-per-*`, `--ttft` or `--decode-rate` flag
        // asked for, and its row would silently report tokens computed at
        // the wrong point while the header above it claimed the flag's.
        totals.ladder_directed.model = model;
        totals
    }

    /// Every arm's totals, in one array, so a fold over all of them does not
    /// have to name each one twice.
    fn arms_mut(&mut self) -> [&mut Aggregate; 24] {
        [
            &mut self.hive_default,
            &mut self.hive_tuned,
            &mut self.hive_refuting,
            &mut self.hive_evidential,
            &mut self.hive_knowing,
            &mut self.hive_deferring,
            &mut self.hive_aside,
            &mut self.hive_ask,
            &mut self.hive_aside_informed,
            &mut self.hive_aside_fact,
            &mut self.hive_aside_mute,
            &mut self.hive_aside_alongside,
            &mut self.hive_aside_exchange,
            &mut self.hive_exchange_rounds,
            &mut self.hive_aside_hush,
            &mut self.hive_exchange_quiet,
            &mut self.hive_aside_offfloor,
            &mut self.hive_pooled,
            &mut self.hive_wide,
            &mut self.hive_blind_wide,
            &mut self.hive_both,
            &mut self.all_reasoning,
            &mut self.vote,
            &mut self.ladder,
        ]
    }

    /// The same array, borrowed.
    fn arms(&self) -> [&Aggregate; 24] {
        [
            &self.hive_default,
            &self.hive_tuned,
            &self.hive_refuting,
            &self.hive_evidential,
            &self.hive_knowing,
            &self.hive_deferring,
            &self.hive_aside,
            &self.hive_ask,
            &self.hive_aside_informed,
            &self.hive_aside_fact,
            &self.hive_aside_mute,
            &self.hive_aside_alongside,
            &self.hive_aside_exchange,
            &self.hive_exchange_rounds,
            &self.hive_aside_hush,
            &self.hive_exchange_quiet,
            &self.hive_aside_offfloor,
            &self.hive_pooled,
            &self.hive_wide,
            &self.hive_blind_wide,
            &self.hive_both,
            &self.all_reasoning,
            &self.vote,
            &self.ladder,
        ]
    }

    /// Fold another chunk of rooms' totals in, arm by arm.
    ///
    /// Called in **room order**, which is the whole contract: see
    /// [`Aggregate::merge`] for why folding out of order would leave every
    /// paired interval in the second table quietly wrong.
    ///
    /// `ladder_directed` sits outside the arrays above because it is the one
    /// arm whose per-room work depends on a directory earned over `--history`
    /// prior episodes of the *same* room, so it is merged explicitly here
    /// rather than being reachable through an index.
    pub(super) fn merge(&mut self, other: &Self) {
        for (mine, theirs) in self.arms_mut().into_iter().zip(other.arms()) {
            mine.merge(theirs);
        }
        self.ladder_directed.merge(&other.ladder_directed);
    }
}
