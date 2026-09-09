//! Simulated participants with noisy private evaluations of a shared task.
//!
//! The room decides between `topic_count` options, exactly one of which is
//! genuinely best. Every participant holds a *private* evaluation of each
//! option — the truth plus noise — so no single participant is reliable and
//! the room's only route to the right answer is to pool what its members
//! independently believe. That is the property the benchmark measures: whether
//! a bounded deliberation aggregates noisy private signals better than the
//! single-responder ladder does, at a comparable turn budget.
//!
//! The agents are deliberately mechanical. A language model would make the
//! numbers unreproducible and would confound protocol quality with model
//! quality; `live` drives the same protocol through a real agent CLI when that
//! is what is wanted.
//!
//! # The evidence-first opening
//!
//! Under `--blind-evidence` a member's first turn, while the room is still
//! [`Visibility::Blind`], is a *deposit* rather than a position: it states its
//! own reading of the topic it knows best and nothing else. Proposals begin
//! once the room goes to [`Visibility::Full`].
//!
//! This is a **participant policy, not a library mechanism**. Nothing in
//! `tinyhivemind-hive` can make a member open this way — `step` decides who
//! speaks, never what they say — and the flag exists because the alternative
//! makes every floor mechanism unreachable. A room whose members share a bias
//! puts the same option on the floor four times inside the blind round, which
//! is already four distinct supporters, which is already quorum: the episode's
//! first non-blind turn is a commit turn and an arriving fact has nothing left
//! to change. That is the same finding the live rooms recorded — in every
//! correct episode of
//! `docs/experiments/2026-09-01-live-hidden-profile.md` the five blind turns
//! were five `!evidence` lines, one per member — and the same one
//! `docs/experiments/2026-09-02-federated-hidden-profile.md` reached from the
//! other side, where moving a desk's question to *before* it had backed
//! anything was the difference between the whole federation failing and 77.5%.
//!
//! So the flag is off by default, and every published number that does not ask
//! for it is unchanged. What it buys, and what it costs, is a result rather
//! than an assumption.
//!
//! [`Visibility::Blind`]: tinyhivemind_hive::Visibility::Blind
//! [`Visibility::Full`]: tinyhivemind_hive::Visibility::Full
//!
//! # Module layout
//!
//! The room and its tuning constants live here; member generation
//! ([`generation`]), the participant itself ([`agent`]), and the read-only
//! projection a turn composes against ([`view`]) each get their own module so
//! that no single file outgrows what a reader can hold in mind at once.

use tinyhivemind_hive::trace::TopicId;

use crate::context::{Compaction, ContextBudget};
use crate::rng::{Rng, mix};

mod agent;
mod generation;
mod view;

use agent::Holdings;
use generation::{
    MemberDraw, draw_expertise, hidden_profile_agent, selfcheck_uniform, specialist_agent,
};

pub(crate) use agent::{CheckStyle, SimAgent};
pub(crate) use view::check_selfcheck;

/// How far a stage decided wrongly lifts the next stage's decoy.
///
/// Bounded on both sides, like `HIDDEN_LIFT` and `GROUNDS_WEIGHT` before it.
/// *Below* the 60-point gap between the true option and a decoy, so a poisoned
/// room can still recover — a chain in which one wrong answer made every later
/// one unwinnable would measure nothing after the first mistake. *Above* zero
/// by enough to matter against the noise, or a chain would merely be a repeat
/// with extra steps. Forty is three quarters of the way to unrecoverable.
pub(crate) const POISON_LIFT: i32 = 40;

/// Names drawn on, in order, for a room's options.
pub(crate) const TOPIC_NAMES: [&str; 8] = [
    "stage", "ship", "revert", "shadow", "canary", "freeze", "split", "pilot",
];

/// The largest room this harness will build, as a `u32`.
///
/// Declared first and widened into [`MAX_MEMBERS`] rather than the other way
/// round, so the refutation cap below is a plain constant rather than a cast
/// that has to argue it cannot truncate.
const MAX_MEMBERS_U32: u32 = 256;

/// The largest room this harness will build.
///
/// Not a property of the library, which has no room-size limit — a bound on
/// what a *benchmark* will spend. Every arm decides the same rooms, so one
/// swept size costs every arm at once, and a room of a thousand members would
/// spend minutes per size to answer a question the shape of the curve already
/// answers by a hundred.
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







    /// What one member's own turn costs, by id.
    ///
    /// A convenience for a caller that only holds an id and not a
    /// [`Participant`](crate::run::Participant) reference -- the vote arm
    /// charges its matched budget this way, one member at a time, without
    /// building a trait object for each. `1` for a member `Room::generate_with`
    /// did not find, the same default every member starts at.
    pub(crate) fn cost_of(&self, agent_id: &str) -> u32 {
        self.agents
            .iter()
            .find(|agent| agent.id == agent_id)
            .map_or(1, |agent| agent.cost_unit)
    }
}
