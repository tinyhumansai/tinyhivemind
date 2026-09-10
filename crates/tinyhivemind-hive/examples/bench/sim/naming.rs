//! What a room's members and options are called, and how many of each this
//! harness will build.
//!
//! Both tables here are eight entries long and both are *starting points*
//! rather than limits. The first eight names in each are the ones every
//! recorded number was written against, so those numbers reproduce exactly
//! rather than approximately; past them both generate. A fixed table is a cap
//! on room size — or on the slate — dressed as a convenience, and both caps
//! were real: `--agents` clamped at 256 and `--topics` at eight, the length of
//! the array below.
//!
//! The two ceilings are bounds on what a *benchmark* will spend, not
//! properties of the library, which places no limit on either.

use tinyhivemind_hive::trace::TopicId;

/// Names drawn on, in order, for a room's first eight options.
///
/// Beyond them [`topic_at`] generates, for the same reason [`member_at`]
/// does: a fixed table is a cap on the slate dressed as a convenience.
pub(super) const TOPIC_NAMES: [&str; 8] = [
    "stage", "ship", "revert", "shadow", "canary", "freeze", "split", "pilot",
];

/// The largest slate this harness will put on the floor.
///
/// The same kind of bound as [`MAX_MEMBERS`]: a limit on what a benchmark
/// will spend, not a property of the library, which places no ceiling on how
/// many topics a transcript may carry.
pub(crate) const MAX_TOPICS: usize = 256;

/// The id of option `index`, for a slate of any size.
///
/// The first eight keep the names every recorded number was written against,
/// so those numbers reproduce exactly rather than approximately. Past them the
/// ids are generated, which is what lets the slate grow with the room: a
/// thousand members choosing between four options is a task a plurality solves
/// by itself, and a benchmark run there measures the law of large numbers
/// rather than the library.
pub(crate) fn topic_at(index: usize) -> TopicId {
    TOPIC_NAMES.get(index).map_or_else(
        || TopicId::from(format!("topic{index}")),
        |name| TopicId::from(*name),
    )
}

/// The largest room this harness will build, as a `u32`.
///
/// Declared first and widened into [`MAX_MEMBERS`] rather than the other way
/// round, so the refutation cap below is a plain constant rather than a cast
/// that has to argue it cannot truncate.
pub(crate) const MAX_MEMBERS_U32: u32 = 1024;

/// The largest room this harness will build.
///
/// Not a property of the library, which has no room-size limit — a bound on
/// what a *benchmark* will spend. Every arm decides the same rooms, so one
/// swept size costs every arm at once, and a swept size of a thousand costs
/// minutes rather than seconds even with the per-room loop spread across
/// every core.
///
/// It was 256 while the recorded tables stopped at 64. A thousand is where the
/// question this harness is now asked stops: what the mechanics do at the
/// scale a real hive mind would run at.
pub(crate) const MAX_MEMBERS: usize = MAX_MEMBERS_U32 as usize;

/// Names and roles drawn on, in order, for a room's first eight members.
///
/// Beyond them [`member_at`] generates, because a fixed table is a cap on room
/// size dressed as a convenience: `--agents` clamped to 8 for no reason other
/// than that this array ends there.
pub(crate) const MEMBER_ROLES: [(&str, Role); 8] = [
    ("planner", Role::Proposer),
    ("critic", Role::Critic),
    ("archivist", Role::Archivist),
    ("scout", Role::Proposer),
    ("auditor", Role::Critic),
    ("historian", Role::Archivist),
    ("builder", Role::Proposer),
    ("reviewer", Role::Critic),
];

/// The name and role of member `index`, for a room of any size.
///
/// The first eight keep the names the recorded benchmarks were written
/// against, so every number at those sizes is reproduced exactly rather than
/// approximately. Past them the roles cycle in the same order — a room of
/// thirty-two is four of the same rotation, which keeps the mix of proposers,
/// critics and archivists flat as the room grows instead of letting one role
/// dominate a large desk by accident.
pub(crate) fn member_at(index: usize) -> (String, Role) {
    MEMBER_ROLES.get(index).map_or_else(
        || {
            let (_, role) = MEMBER_ROLES[index % MEMBER_ROLES.len()];
            (format!("seat{index}"), role)
        },
        |(name, role)| ((*name).to_string(), *role),
    )
}

/// How a participant fills a turn it has no strong move for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Role {
    /// Puts its own best option on the floor early.
    Proposer,
    /// Objects to a leading option it privately rates poorly.
    Critic,
    /// Supplies grounds without taking a side.
    Archivist,
}
