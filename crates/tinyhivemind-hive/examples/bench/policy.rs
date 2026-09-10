//! Episode policy construction for the bench binary.
//!
//! Each arm the comparison runs is a small variation on one tuned policy —
//! refutation on, the directory folded, `!defer` bounded, both at once — and
//! keeping the variations together here makes the relationship between arms
//! visible in one place rather than scattered through `compare.rs`'s totals.
//! [`turn_budget`] and [`quorum_threshold`] are the two knobs every policy
//! here scales with the size of the desk.

use tinyhivemind_hive::{DirectoryPolicy, EpisodePolicy, QuorumPolicy};

/// The crate's own conservative default, with the window widened to cover a
/// whole episode so the two hive arms differ only in the knobs the sweep moved.
pub(crate) fn default_policy() -> EpisodePolicy {
    EpisodePolicy {
        round_width: SEQUENTIAL,
        revealed_width: SEQUENTIAL,
        quorum: QuorumPolicy {
            window: 100,
            ..QuorumPolicy::DEFAULT
        },
        ..EpisodePolicy::DEFAULT
    }
}

/// The width every published arm runs at.
///
/// `EpisodePolicy::DEFAULT` runs wider rounds, because a seat is an async
/// session. Every number recorded before ADR 0014 was measured at width one,
/// and an arm that silently changed width would make a comparison against
/// those numbers meaningless — so the published arms ask for width one and the
/// concurrency arms ask for what they are testing. `--round-width` overrides
/// it, and `docs/experiments/` carries what the wider rounds scored.
pub(crate) const SEQUENTIAL: u32 = 1;

/// The policy `--sweep` picks, scaled to the size of the desk.
///
/// The load-bearing knob is the quorum threshold, and it has two bounds rather
/// than one.
///
/// A threshold *above half the desk* is what suppresses deadlock. Five members
/// can put two grounded supporters behind each of two options, and an episode
/// in which two options both carry is deadlocked by definition: no amount of
/// further support resolves it, because both stay above the line. Requiring a
/// majority makes two *disjoint* supporter sets impossible, and the measured
/// deadlock rate falls to zero.
///
/// It does not make deadlock unreachable in general — one member may back two
/// topics, and a member in both supporter sets is not two members. It is
/// unreachable for the simulated participants here, which each back one
/// option. See `QuorumPolicy::for_room`.
///
/// A threshold *below the whole desk* is what keeps a decision reachable.
/// Cross-inhibition removes a silenced advocate from a topic's supporter set
/// and does not put them back, so at unanimity a single grounded `!object`
/// makes quorum unreachable for the rest of the episode. A live three-member
/// room ran into exactly that and spent its whole budget without deciding.
///
/// Between the two: the smallest majority of the desk, and never the whole of
/// it.
pub(crate) fn tuned_policy(agents: usize) -> EpisodePolicy {
    EpisodePolicy {
        turn_budget: turn_budget(agents),
        round_width: SEQUENTIAL,
        revealed_width: SEQUENTIAL,
        blind_round: true,
        dominance_cap: 40,
        repetition_cap: 2,
        quorum: QuorumPolicy {
            threshold: quorum_threshold(agents),
            window: 100,
            require_grounded: true,
            // Refutation is off in both control arms, which is the crate
            // default. No topic is ever capped *and* the simulated members
            // never spend a turn on the move, so `hive+ref` differs from
            // `hive+` in exactly one thing rather than in two.
            refutation_cap: None,
            ..QuorumPolicy::DEFAULT
        },
        ..EpisodePolicy::DEFAULT
    }
}

/// The tuned policy with the negative evidence-to-topic link switched on.
///
/// A cap of two is the crate default: one member's assertion should not kill a
/// hypothesis, and two distinct grounded refuters should. This is the arm the
/// mechanism has to earn its place against, and it can lose.
pub(crate) fn refuting_policy(tuned: &EpisodePolicy) -> EpisodePolicy {
    EpisodePolicy {
        quorum: QuorumPolicy {
            refutation_cap: Some(2),
            ..tuned.quorum
        },
        ..*tuned
    }
}

/// The tuned policy with the folded transactive-memory directory switched on.
///
/// One field moves. `directory: Some(DirectoryPolicy::DEFAULT)` is what makes
/// [`BidReason::Knows`] reachable at all: without it there is no contested
/// topic and no holder to promote, so the attention market routes on recency
/// and trace importance exactly as it did before. Nothing else about the
/// episode changes, which is what lets the difference between this arm and
/// `hive+` be attributed to the directory rather than to a second knob.
///
/// [`BidReason::Knows`]: tinyhivemind_hive::BidReason::Knows
pub(crate) fn knowing_policy(tuned: &EpisodePolicy) -> EpisodePolicy {
    EpisodePolicy {
        directory: Some(DirectoryPolicy::DEFAULT),
        ..*tuned
    }
}

/// The tuned policy with `!defer` bounded at `cap`, and no directory.
///
/// This is the honest control for the deferring arm: a room where members may
/// stand aside on a topic that is not theirs, but where nothing folds a
/// directory to route the vacated turn anywhere in particular. If `!defer`
/// pays for itself only in the presence of a directory, that is worth knowing
/// separately from whether it pays for itself at all.
pub(crate) fn deferring_policy(tuned: &EpisodePolicy, cap: u32) -> EpisodePolicy {
    EpisodePolicy {
        defer_cap: Some(cap.max(1)),
        ..*tuned
    }
}

/// The tuned policy widened, so a round authorizes several turns at once.
///
/// This is the arm ADR 0014 has to earn its place against, and it can lose.
/// The prediction it tests is that a wider round buys **depth** — a host with
/// async seats waits once for the whole round — without buying correlated
/// error, because members writing at the same time cannot read each other and
/// so a concurrent round is a blind round. If accuracy falls, the loss is the
/// price of the depth and the table says so.
///
/// A width of `0` returns the tuned policy unchanged, which makes the arm
/// bit-identical to `hive+` — the discipline `set_aside_cap` and
/// `--exchange-cap` already follow.
pub(crate) fn widened_policy(tuned: &EpisodePolicy, width: u32) -> EpisodePolicy {
    if width == 0 {
        return *tuned;
    }
    EpisodePolicy {
        round_width: width,
        revealed_width: width,
        ..*tuned
    }
}

/// The tuned policy widened **only while the room is blind**.
///
/// The free half of concurrency, on its own. A blind member cannot read a
/// peer's row whether or not it runs concurrently with that peer, so this arm
/// should score exactly what `hive+` scores while waiting fewer times. It is
/// the control that separates the depth a round buys from the information a
/// wide *revealed* round spends.
pub(crate) fn blind_wide_policy(tuned: &EpisodePolicy, width: u32) -> EpisodePolicy {
    if width == 0 {
        return *tuned;
    }
    EpisodePolicy {
        round_width: width,
        revealed_width: SEQUENTIAL,
        ..*tuned
    }
}

/// Both mechanisms at once: the directory folded, and `!defer` bounded.
///
/// This is the arrangement `docs/specs/expert-delegation.md` describes end to
/// end — a member says "not mine", that promotes the topic to the contested
/// one, and the directory decides who the vacated turn goes to.
pub(crate) fn knowing_deferring_policy(tuned: &EpisodePolicy, cap: u32) -> EpisodePolicy {
    EpisodePolicy {
        directory: Some(DirectoryPolicy::DEFAULT),
        defer_cap: Some(cap.max(1)),
        ..*tuned
    }
}

/// The refuting policy with grounds weighed by evidential depth as well.
pub(crate) fn evidential_policy(tuned: &EpisodePolicy) -> EpisodePolicy {
    let refuting = refuting_policy(tuned);
    EpisodePolicy {
        quorum: QuorumPolicy {
            require_evidential: true,
            ..refuting.quorum
        },
        ..refuting
    }
}

/// Turns a desk needs to reach a majority quorum.
///
/// A blind opening round costs one turn per member before anybody has seen
/// anybody, a majority then has to assemble on one option, and the decision
/// has to be recorded. Three turns per member covers that with room for the
/// objections and questions a real room spends turns on, and it is a cap
/// rather than a cost: a five-member room finishes in under seven turns of the
/// fifteen it is allowed. A budget that does not scale with the desk is what
/// makes a larger room look worse than a smaller one — at a fixed twelve, an
/// eight-member room fails to decide a third of the time and scores 64%; at
/// twenty-four it decides 96% of the time and scores 88%.
pub(crate) fn turn_budget(agents: usize) -> u32 {
    u32::try_from(agents).unwrap_or(5).saturating_mul(3).max(6)
}

/// The smallest majority of a desk that still leaves one member to spare.
pub(crate) fn quorum_threshold(agents: usize) -> u32 {
    let agents = u32::try_from(agents).unwrap_or(u32::MAX);
    let majority = agents / 2 + 1;
    majority.min(agents.saturating_sub(1)).max(2)
}
