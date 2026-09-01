//! A grid search over episode policy.
//!
//! The policy is the tuning surface a host actually has: how long a room may
//! run, how many grounded supporters a decision needs, whether the opening
//! round is blind, and how hard dominance and repetition are damped. The sweep
//! scores each combination on the same rooms and reports the ordering, so a
//! host can pick a policy from evidence rather than from taste.

use tinyhivemind_hive::{EpisodePolicy, QuorumPolicy};

use crate::metrics::Aggregate;
use crate::run::run_episode;
use crate::sim::Room;

/// Values swept per field.
///
/// Budget and quorum are swept *relative to the desk*, because both only mean
/// anything against a room size: twelve turns is generous for three members
/// and not enough for eight, and a threshold of three is a bare majority of
/// five and unanimity of three.
const BUDGET_PER_MEMBER: [u32; 3] = [2, 3, 4];
const BLIND: [bool; 2] = [true, false];
const REPETITION: [u32; 2] = [2, 3];
const DOMINANCE: [u32; 2] = [40, 60];
const WINDOWS: [u32; 2] = [30, 100];
/// Refutation off, then at each cap a five-member room could actually reach.
///
/// `None` is the crate default and the control: no topic is capped, and the
/// simulated members do not spend a turn on a move that cannot take effect.
const REFUTATION: [Option<u32>; 3] = [None, Some(2), Some(3)];
const EVIDENTIAL: [bool; 2] = [false, true];

/// The quorum thresholds worth trying for a desk of `agents`: two, the
/// smallest majority, and unanimity.
fn thresholds(agents: usize) -> Vec<u32> {
    let agents = u32::try_from(agents).unwrap_or(5);
    let mut thresholds = vec![2, agents / 2 + 1, agents];
    thresholds.sort_unstable();
    thresholds.dedup();
    thresholds
}

/// One scored point of the grid.
#[derive(Clone, Debug)]
pub(crate) struct Scored {
    /// The policy that was run.
    pub(crate) policy: EpisodePolicy,
    /// What it achieved.
    pub(crate) totals: Aggregate,
}

impl Scored {
    /// Rank a point: correctness first, then reaching a decision at all, then
    /// spending fewer turns to do it.
    ///
    /// The key is integer throughout, so the ordering is exact and the sweep
    /// reports the same winner on every machine.
    fn key(&self) -> (u64, u64, i64) {
        let episodes = u64::from(self.totals.episodes).max(1);
        let accuracy = u64::from(self.totals.correct) * 10_000 / episodes;
        let decided = u64::from(self.totals.converged) * 10_000 / episodes;
        let turns = self.totals.turns * 100 / episodes;
        (accuracy, decided, -i64::try_from(turns).unwrap_or(i64::MAX))
    }
}

/// Score every policy in the grid over the same sample of rooms.
///
/// # Errors
///
/// Propagates any library error, which here can only mean a malformed policy.
pub(crate) fn sweep(rooms: &[Room], task: &str, agents: usize) -> Result<Vec<Scored>, String> {
    let per_member = u32::try_from(agents).unwrap_or(5);
    let mut scored = Vec::new();
    for multiple in BUDGET_PER_MEMBER {
        let budget = per_member.saturating_mul(multiple).max(6);
        for threshold in thresholds(agents) {
            for blind in BLIND {
                for repetition in REPETITION {
                    for dominance in DOMINANCE {
                        for window in WINDOWS {
                            for refutation_cap in REFUTATION {
                                for require_evidential in EVIDENTIAL {
                                    let policy = EpisodePolicy {
                                        turn_budget: budget,
                                        blind_round: blind,
                                        dominance_cap: dominance,
                                        repetition_cap: repetition,
                                        quorum: QuorumPolicy {
                                            threshold,
                                            window,
                                            require_grounded: true,
                                            refutation_cap,
                                            require_evidential,
                                        },
                                        ..EpisodePolicy::DEFAULT
                                    };
                                    let mut totals = Aggregate::default();
                                    for room in rooms {
                                        totals.add(&run_episode(room, &policy, task, false)?);
                                    }
                                    scored.push(Scored { policy, totals });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    scored.sort_by_key(|point| std::cmp::Reverse(point.key()));
    Ok(scored)
}

/// The header for the sweep table.
pub(crate) fn header() -> String {
    format!(
        "{:>7}{:>7}{:>7}{:>7}{:>7}{:>8}{:>8}{:>7}{:>11}{:>11}{:>10}",
        "budget",
        "quorum",
        "window",
        "blind",
        "domin",
        "repeat",
        "refute",
        "evid",
        "decided %",
        "correct %",
        "turns/ep"
    )
}

/// One row of the sweep table.
pub(crate) fn row(point: &Scored) -> String {
    format!(
        "{:>7}{:>7}{:>7}{:>7}{:>7}{:>8}{:>8}{:>7}{:>11.1}{:>11.1}{:>10.2}",
        point.policy.turn_budget,
        point.policy.quorum.threshold,
        point.policy.quorum.window,
        if point.policy.blind_round {
            "yes"
        } else {
            "no"
        },
        point.policy.dominance_cap,
        point.policy.repetition_cap,
        point
            .policy
            .quorum
            .refutation_cap
            .map_or_else(|| "off".to_owned(), |cap| cap.to_string()),
        if point.policy.quorum.require_evidential {
            "yes"
        } else {
            "no"
        },
        point.totals.decision_rate(),
        point.totals.accuracy(),
        point.totals.turns_per_episode(),
    )
}
