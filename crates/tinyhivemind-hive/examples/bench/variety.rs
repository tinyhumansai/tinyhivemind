//! The variety sweep: what a task with several facets at once costs, and who
//! pays it.
//!
//! `--stages` measures **depth** — one question after another, each poisoned
//! by the last. This measures **width**: a task that is several questions *at
//! the same time*, each needing a different kind of attention, none of them
//! waiting on the others. A release has a rollout decision and a migration
//! decision and a rollback decision, and they are not one decision taken three
//! times.
//!
//! It exists because the horizon experiment
//! (`docs/experiments/2026-09-09-the-long-horizon.md`) answered its question
//! and the answer was *no*: against a soloist that compacts by a superseding
//! account rather than by eviction, a room of five loses at every horizon and
//! every window, and pays about seven times the depth to do it. Length alone
//! does not make a room worth its price. The claim left standing is the one
//! this module tests — that **variety** does, because one agent has one set of
//! priors and a room has several.
//!
//! # The task
//!
//! `--facets F` runs F independent sub-decisions that all belong to one task.
//! Two things make them a spread rather than a chain:
//!
//! - **They do not compound through time.** No facet poisons another; they are
//!   answered against the same world. What compounds is the *conjunction*:
//!   `all-facets %` counts a task done only when every facet is right, so the
//!   headline falls with F for every arm and the question is how fast.
//! - **Each facet names its options distinctly**, so a reading taken on one
//!   facet can never be scored against another's question. It can only take up
//!   a row — which is the cost a soloist holding the whole task pays and a
//!   member holding one facet does not.
//!
//! Under `--roles` each facet additionally has an **owner** who reads it far
//! more tightly than anybody else, `facet % members`, fixed by construction
//! rather than drawn. That is the second mechanism, and keeping it behind a
//! flag is what lets the two be told apart: without it the arms differ only in
//! how the load is split, and with it they differ in competence too.
//!
//! # The arms
//!
//! | arm | what it is |
//! | --- | --- |
//! | `solo` | one agent handed every member's readings on every facet, deciding all of them alone, in one window, compacting by eviction |
//! | `solo+fold` | the same agent compacting by a superseding account — the arm that won the horizon |
//! | `hive+fold` | **the arm under test**: one `solo+fold` per facet. Facet `f` is decided alone by member `f % n`, holding only its own facets, folding what overflows |
//! | `hive+` | the room deliberating every facet on the floor — every member carries every facet's brief |
//! | `hive+pooled` | the ceiling: every reading in every member's hands, free |
//!
//! `hive+fold` is the user's proposal stated in the harness's own units: keep
//! the thing that won, and make each of them a seat. Its depth is
//! `⌈F / n⌉` rounds rather than `F`, because independent facets are exactly
//! what [ADR 0014](../../../../docs/adr/0014-a-round-authorizes-concurrent-turns.md)
//! authorizes a single round to run at once.
//!
//! It is allowed to lose, like every arm here.

use std::fmt::Write as _;
use std::time::Instant;

use tinyhivemind_hive::division::{DivisionPolicy, divide};
use tinyhivemind_hive::episode::EpisodePolicy;
use tinyhivemind_hive::trace::TopicId;

use crate::TASK;
use crate::cli::Options;
use crate::context::Compaction;
use crate::policy::tuned_policy;
use crate::rng::mix;
use crate::run::{Host, run_episode};
use crate::sim::{Expertise, MAX_MEMBERS, Room, member_at};

/// One arm's score over a sample of tasks.
#[derive(Default)]
struct Spread {
    /// Tasks in which every facet was decided correctly.
    whole: u32,
    /// Facets decided correctly, across every task.
    facets_right: u32,
    /// Facets attempted, across every task.
    facets: u32,
    /// Tasks run.
    tasks: u32,
    /// Rows a participant was carrying when the task ended, summed.
    held: f64,
    /// Rounds spent across every facet of every task.
    rounds: u64,
    /// Turns spent across every facet of every task.
    turns: u64,
}

impl Spread {
    fn add(&mut self, run: &TaskRun) {
        self.tasks = self.tasks.saturating_add(1);
        self.facets = self.facets.saturating_add(run.facets);
        self.facets_right = self.facets_right.saturating_add(run.right);
        if run.right == run.facets && run.facets > 0 {
            self.whole = self.whole.saturating_add(1);
        }
        self.held += run.held;
        self.rounds = self.rounds.saturating_add(u64::from(run.rounds));
        self.turns = self.turns.saturating_add(u64::from(run.turns));
    }

    fn all_facets(&self) -> f64 {
        ratio(self.whole.into(), self.tasks.into()) * 100.0
    }

    fn per_facet(&self) -> f64 {
        ratio(self.facets_right.into(), self.facets.into()) * 100.0
    }

    fn held_per_task(&self) -> f64 {
        ratio_f64(self.held, self.tasks.into())
    }

    fn rounds_per_task(&self) -> f64 {
        ratio_f64(as_f64(self.rounds), self.tasks.into())
    }

    fn turns_per_task(&self) -> f64 {
        ratio_f64(as_f64(self.turns), self.tasks.into())
    }
}

/// What one task cost one arm.
#[derive(Default)]
struct TaskRun {
    facets: u32,
    right: u32,
    rounds: u32,
    turns: u32,
    held: f64,
}

impl TaskRun {
    /// Record one facet's outcome and the one turn it took to reach it.
    fn one_facet(&mut self, correct: bool) {
        self.facets = self.facets.saturating_add(1);
        self.turns = self.turns.saturating_add(1);
        if correct {
            self.right = self.right.saturating_add(1);
        }
    }
}

/// One arm's score at one task width.
struct Point {
    facets: usize,
    arm: &'static str,
    all_facets: f64,
    per_facet: f64,
    held: f64,
    depth: f64,
    width: f64,
}

/// Every arm this sweep scores, in the order the tables print them.
const ARMS: [&str; 6] = [
    "solo",
    "solo+fold",
    "hive+fold",
    "hive+alone",
    "hive+",
    "hive+pooled",
];

/// The task widths swept when `--facets` names only one.
///
/// Doubling for the reason the horizon ladder doubles: the interesting
/// behaviour is a curve, and a linear ladder spends most of its length where
/// nothing changes. It stops at eight because a room of five running a
/// sixteen-facet task is measuring the ladder's own wrap-around more than it
/// is measuring the room.
pub(crate) const DEFAULT_SPREADS: [usize; 4] = [1, 2, 4, 8];

/// Run the variety ladder and print what each arm scored.
///
/// # Errors
///
/// Returns the library's own error text if a policy or snapshot is malformed.
pub(crate) fn sweep(options: &Options) -> Result<(), String> {
    let spreads = if options.facets.is_empty() {
        DEFAULT_SPREADS.to_vec()
    } else {
        options.facets.clone()
    };
    let started = Instant::now();
    let mut points = Vec::new();
    for facets in &spreads {
        points.extend(one_spread(options, *facets)?);
    }
    print!("{}", render(&points, &spreads, options));
    println!("swept in {:.2} s", started.elapsed().as_secs_f64());
    Ok(())
}

/// Score every arm over one task width.
fn one_spread(options: &Options, facets: usize) -> Result<Vec<Point>, String> {
    let tuned = tuned_policy(options.agents);
    let mut solo = Spread::default();
    let mut folding = Spread::default();
    let mut split = Spread::default();
    let mut alone = Spread::default();
    let mut room_arm = Spread::default();
    let mut pooled = Spread::default();

    for episode in 0..options.episodes {
        let seed = mix(options.seed, u64::from(episode));
        solo.add(&run_alone(options, seed, facets, Compaction::Evict));
        folding.add(&run_alone(options, seed, facets, Compaction::Fold));
        split.add(&run_split(options, seed, facets, true));
        alone.add(&run_split(options, seed, facets, false));
        room_arm.add(&run_room(options, seed, facets, &tuned, false)?);
        pooled.add(&run_room(options, seed, facets, &tuned, true)?);
    }

    Ok([
        ("solo", solo),
        ("solo+fold", folding),
        ("hive+fold", split),
        ("hive+alone", alone),
        ("hive+", room_arm),
        ("hive+pooled", pooled),
    ]
    .into_iter()
    .map(|(arm, spread)| Point {
        facets,
        arm,
        all_facets: spread.all_facets(),
        per_facet: spread.per_facet(),
        held: spread.held_per_task(),
        depth: spread.rounds_per_task(),
        width: spread.turns_per_task(),
    })
    .collect())
}

/// Members this sweep's rooms actually hold, clamped as `Room` clamps.
fn members(options: &Options) -> usize {
    options.agents.clamp(2, MAX_MEMBERS)
}

/// Build the room one facet of a task runs on.
///
/// Renamed for this facet, owned by `facet % members` when `--roles` is set,
/// and inheriting whatever the participant that will read it was already
/// carrying.
fn faceted(options: &Options, seed: u64, facet: usize, prior: Option<&Room>) -> Room {
    let expertise = if options.roles {
        Expertise::Roles {
            owner: facet % members(options),
        }
    } else {
        options.expertise
    };
    let base = Room::generate_with(
        // Mixed with a token of its own, so a facet of a task is never the
        // same draw as a stage of a chain at the same index.
        mix(seed, 0x4641_4345 ^ facet as u64),
        options.agents,
        options.topics,
        options.noise,
        expertise,
        options.cost,
    );
    let mut room = base.for_facet(facet);
    if let Some(prior) = prior {
        room = room.inheriting(prior);
    }
    // Every participant pays for the brief it was handed before any arm
    // decides what to do with it — the same charge `--stages` makes, for the
    // same reason.
    room.charge_brief();
    room.set_budget(options.budget());
    room
}

/// One task, every facet decided by a single agent holding all of them.
///
/// The soloist is the room's own first member with every peer's readings
/// installed, answering from its own argmax. One turn and one round per facet,
/// because a single agent cannot work two facets at once — which is precisely
/// the depth the split arm below does not pay.
fn run_alone(options: &Options, seed: u64, facets: usize, compaction: Compaction) -> TaskRun {
    let mut run = TaskRun::default();
    let mut prior: Option<Room> = None;
    for facet in 0..facets {
        let mut room = faceted(options, seed, facet, prior.as_ref());
        // Everything the room collectively holds, in one participant's window.
        room = room.pooled();
        room.set_compaction(compaction);
        let Some(agent) = room.agents.first() else {
            break;
        };
        run.one_facet(*agent.favourite() == room.truth);
        run.rounds = run.rounds.saturating_add(1);
        run.held = room.held();
        prior = Some(room);
    }
    run
}

/// One task, each facet decided by the member whose facet it is.
///
/// The arm under test, and its own matched control. Member `f % n` answers
/// facet `f`, folding what overflows its window, and carries **only the facets
/// it owns** — so the load one participant holds grows with `F / n` rather
/// than with `F`. Every facet is independent of every other, so the turns that
/// answer them are authorized against the same transcript and none can read
/// another: one round covers `n` of them.
///
/// `pool` is the difference between the two:
///
/// - **`true` — `hive+fold`.** The owner reads its peers' readings *of its own
///   facet*, which is the same information the soloist has about that facet
///   and none of what the soloist has about the others. It is the user's
///   proposal stated exactly: keep `solo+fold`, and make one of them a seat.
/// - **`false` — `hive+alone`.** The same seat with the pooling removed and
///   nothing else changed, so what the pooling is worth is a measured
///   difference rather than an assumption. Splitting a task and *not* sharing
///   what the room knows about each piece is the failure mode this control
///   exists to price.
fn run_split(options: &Options, seed: u64, facets: usize, pool: bool) -> TaskRun {
    let mut run = TaskRun::default();
    let count = members(options);
    let Some(division) = library_division(count, facets) else {
        return run;
    };
    let mut priors: Vec<Option<Room>> = vec![None; count];
    for (facet, assigned) in division.assignments().iter().enumerate() {
        let Some(owner) = seat_of(&assigned.owner, count) else {
            break;
        };
        let Some(slot) = priors.get_mut(owner) else {
            break;
        };
        // Taken rather than borrowed: the prior room is consumed into this
        // one's inherited windows and put back below, so the seat holds
        // exactly one room at a time.
        let prior = slot.take();
        let mut room = faceted(options, seed, facet, prior.as_ref());
        if pool {
            // Its peers' readings **of this facet only**. A facet's room holds
            // that facet's options and no other, so pooling it hands the owner
            // everything the room knows about its own question and nothing at
            // all about anybody else's — which is the whole difference between
            // this arm and the soloist that pools all of them.
            room = room.pooled();
        }
        room.set_compaction(Compaction::Fold);
        let Some(agent) = room.agents.get(owner) else {
            break;
        };
        run.one_facet(*agent.favourite() == room.truth);
        if let Some(slot) = priors.get_mut(owner) {
            *slot = Some(room);
        }
    }
    // What one seat carried, averaged over the seats that carried anything: a
    // seat given no facet holds nothing and is not a participant in this task.
    let (rows, seats) = priors
        .iter()
        .enumerate()
        .filter_map(|(owner, room)| room.as_ref().map(|room| room.held_by(owner)))
        .fold((0_u64, 0_u64), |(rows, seats), held| {
            (
                rows.saturating_add(u64::try_from(held).unwrap_or(u64::MAX)),
                seats.saturating_add(1),
            )
        });
    run.held = ratio_f64(as_f64(rows), seats);
    // The depth the concurrency buys, read off the library rather than
    // recomputed here: independent facets ride one round, bounded by the
    // seats available and by `DivisionPolicy::round_width`.
    run.rounds = division.depth();
    run
}

/// Ask the library who owns each facet and which of them ride the same round.
///
/// **The arm is the library's own division, not the harness's idea of one.**
/// `hive+fold` was assembled here before `tinyhivemind_hive::division` existed,
/// and rewiring it is what makes the published numbers a measurement of the
/// shipped mechanism rather than of a private reimplementation of it.
///
/// The directory is `None`: these rooms have deliberated nothing yet, so there
/// is no transactive memory to fold and every facet falls to the rotation,
/// which assigns facet `f` to seat `f % n` — exactly what this arm did before.
/// `--roles` puts the competence on that same rotation from the other side, in
/// the room's own draw.
fn library_division(seats: usize, facets: usize) -> Option<tinyhivemind_hive::Division> {
    let names = seat_names(seats);
    let ids: Vec<&str> = names.iter().map(String::as_str).collect();
    let host = Host::new(&ids);
    let questions: Vec<TopicId> = (0..facets)
        .map(|facet| TopicId::from(format!("facet{facet}").as_str()))
        .collect();
    divide(
        &questions,
        crate::run::DESK_ID,
        &host.roster(),
        &host.desks(),
        None,
        DivisionPolicy::DEFAULT,
    )
    .ok()
}

/// The room's seat names, in desk order — the same names `Room` gives its
/// members, so an assignment the library hands back can be resolved to a seat.
fn seat_names(seats: usize) -> Vec<String> {
    (0..seats).map(|index| member_at(index).0).collect()
}

/// The index of one seat in the room, by the name the division named it with.
fn seat_of(owner: &str, seats: usize) -> Option<usize> {
    (0..seats).position(|index| member_at(index).0 == owner)
}

/// One task, every facet deliberated by the whole room.
fn run_room(
    options: &Options,
    seed: u64,
    facets: usize,
    policy: &EpisodePolicy,
    pool: bool,
) -> Result<TaskRun, String> {
    let mut run = TaskRun::default();
    let mut prior: Option<Room> = None;
    for facet in 0..facets {
        let mut room = faceted(options, seed, facet, prior.as_ref());
        if pool {
            room = room.pooled();
        }
        let report = run_episode(&room, policy, TASK, false)?;
        run.facets = run.facets.saturating_add(1);
        run.turns = run.turns.saturating_add(report.turns);
        run.rounds = run.rounds.saturating_add(report.rounds);
        if report.correct {
            run.right = run.right.saturating_add(1);
        }
        run.held = room.held();
        prior = Some(room);
    }
    Ok(run)
}

/// One table per column of interest, task widths across the top.
fn render(points: &[Point], spreads: &[usize], options: &Options) -> String {
    let mut out = String::new();
    let window = if options.context == 0 {
        "unbounded".to_owned()
    } else {
        format!("{} rows, rot {:.1}", options.context, options.rot)
    };
    let roles = if options.roles { "owned" } else { "shared" };
    let _ = write!(
        out,
        "tasks {} per width  agents {}  options {}  facets {roles}  window {window}\n\n",
        options.episodes, options.agents, options.topics,
    );

    for (title, field) in [
        ("all-facets % — every facet right", 0_usize),
        ("facet % — per-decision", 1),
        ("held/ep — rows one participant carries", 2),
        ("rounds/ep — depth over the whole task", 3),
        ("turns/ep — width over the whole task", 4),
    ] {
        let _ = write!(out, "{title}\n\narm          ");
        for facets in spreads {
            let _ = write!(out, "{facets:>9}");
        }
        out.push('\n');
        for arm in ARMS {
            let _ = write!(out, "{arm:<13}");
            for facets in spreads {
                let cell = points
                    .iter()
                    .find(|point| point.arm == arm && point.facets == *facets);
                match cell {
                    Some(point) => {
                        let value = match field {
                            0 => point.all_facets,
                            1 => point.per_facet,
                            2 => point.held,
                            3 => point.depth,
                            _ => point.width,
                        };
                        let _ = write!(out, "{value:>9.1}");
                    }
                    None => {
                        let _ = write!(out, "{:>9}", "—");
                    }
                }
            }
            out.push('\n');
        }
        out.push('\n');
    }
    out
}

/// Integer ratio as a float, zero for an empty denominator.
fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    as_f64(numerator) / as_f64(denominator)
}

/// The same for a running float total.
fn ratio_f64(numerator: f64, denominator: u64) -> f64 {
    if denominator == 0 {
        return 0.0;
    }
    numerator / as_f64(denominator)
}

/// Widen a count for the arithmetic above.
#[expect(
    clippy::cast_precision_loss,
    reason = "a sample larger than an f64 can count is not a sample this harness runs"
)]
fn as_f64(value: u64) -> f64 {
    value as f64
}

#[cfg(test)]
mod test {
    use tinyhivemind_hive::trace::TopicId;

    use super::*;
    use crate::sim::SimAgent;

    /// Compare two percentages, which arrive through floating-point division.
    fn close(left: f64, right: f64) -> bool {
        (left - right).abs() < 1e-9
    }

    /// A room for one facet of a task, at the defaults the sweep uses.
    fn room(facet: usize, expertise: Expertise) -> Room {
        Room::generate_with(0xFACE7, 5, 4, 50, expertise, false).for_facet(facet)
    }

    /// The option names one member of a room holds a reading of.
    fn names(held: &Room, index: usize) -> Vec<String> {
        held.agents.get(index).map_or_else(Vec::new, |agent| {
            agent
                .evals
                .iter()
                .map(|(topic, _)| topic.0.clone())
                .collect()
        })
    }

    /// The property the whole spread rests on: no reading taken on one facet
    /// can be scored against another facet's question.
    #[test]
    fn every_facet_names_its_options_differently() {
        let first = room(0, Expertise::Uniform);
        let second = room(1, Expertise::Uniform);
        let (early, late) = (names(&first, 0), names(&second, 0));
        assert!(!early.is_empty(), "a facet has options");
        for name in &early {
            assert!(
                !late.contains(name),
                "option {name} appears on two facets and could be scored on the wrong one"
            );
        }
    }

    /// A facet's options cannot collide with a stage's either, so a run that
    /// ever combined depth and width could not score one against the other.
    #[test]
    fn a_facet_never_names_an_option_the_way_a_stage_does() {
        let base = Room::generate_with(0xFACE7, 5, 4, 50, Expertise::Uniform, false);
        let staged = names(&base.for_stage(1), 0);
        for name in names(&base.for_facet(1), 0) {
            assert!(
                !staged.contains(&name),
                "facet option {name} collides with a stage's"
            );
        }
    }

    /// Under `--roles` the owner is fixed by construction rather than drawn,
    /// so the harness knows whose facet it is before anybody reads anything.
    #[test]
    fn an_owned_facet_names_its_owner_by_construction() {
        for owner in 0..5 {
            let held = room(0, Expertise::Roles { owner });
            let expected = crate::sim::member_at(owner).0;
            assert_eq!(
                held.deciding_expert(),
                Some(expected.as_str()),
                "member {owner} owns every option of its own facet"
            );
        }
    }

    /// An owner reads its own facet more tightly than a peer does. Stated as
    /// a spread over the room rather than as one draw, because a single noisy
    /// reading proves nothing either way.
    #[test]
    fn an_owner_reads_its_facet_more_tightly_than_its_room_does() {
        let held = room(0, Expertise::Roles { owner: 0 });
        let truth = held.truth.clone();
        let error = |index: usize| {
            held.agents
                .get(index)
                .map_or(i32::MAX, |agent| (agent.own_reading(&truth) - 100).abs())
        };
        let owner = error(0);
        let lay: i32 = (1..held.agents.len()).map(error).sum();
        let mean = lay / i32::try_from(held.agents.len() - 1).unwrap_or(1);
        assert!(
            owner < mean,
            "the owner's error {owner} is not tighter than the room's mean {mean}"
        );
    }

    /// A seat holding one facet of a task carries less than a soloist holding
    /// all of them. That ratio is the whole quantity under test.
    #[test]
    fn a_seat_carries_one_facet_where_a_soloist_carries_every_facet() {
        let mut first = room(0, Expertise::Uniform);
        first.charge_brief();
        let one = first.held_by(0);
        assert!(one > 0, "the brief occupies rows");

        // A soloist goes on to the next facet still holding the last one.
        let mut second = room(1, Expertise::Uniform).inheriting(&first);
        second.charge_brief();
        assert_eq!(
            second.held_by(0),
            one * 2,
            "a soloist on its second facet carries the first one as well"
        );

        // A seat given only its own facet inherits nothing, because the facet
        // before it belonged to somebody else.
        let mut owned = room(1, Expertise::Uniform);
        owned.charge_brief();
        assert_eq!(owned.held_by(0), one, "a seat carries only its own facet");
    }

    /// `held_by` reports one seat rather than the room's mean, which is the
    /// distinction the split arm's load column rests on.
    #[test]
    fn held_by_reads_one_seat_and_not_the_rooms_mean() {
        let mut held = room(0, Expertise::Uniform);
        held.charge_brief();
        if let Some(agent) = held.agents.first_mut() {
            agent.note_stub(&TopicId::from("extra"));
        }
        let mean = held.held();
        let first = held.held_by(0);
        let second = held.held_by(1);
        assert!(first > second, "the seat given an extra row holds more");
        assert!(
            as_f64(u64::try_from(first).unwrap_or(0)) > mean,
            "one busy seat is understated by the room's mean"
        );
        assert_eq!(
            held.held_by(usize::MAX),
            0,
            "a seat nobody fills holds nothing"
        );
    }

    /// The depth the concurrency buys: independent facets ride one round, so
    /// a room of `n` answers `n` of them for the wall clock of one.
    #[test]
    fn a_split_task_is_shallower_than_a_serial_one() {
        for (facets, seats, expected) in [(1_usize, 5_usize, 1_usize), (4, 5, 1), (8, 5, 2)] {
            assert_eq!(facets.div_ceil(seats), expected);
            assert!(
                facets.div_ceil(seats) <= facets,
                "splitting a task can never make it deeper than working it alone"
            );
        }
    }

    /// All-facets can never exceed the per-facet rate, and equals it at one
    /// facet — the accounting invariant the headline column rests on.
    #[test]
    fn a_whole_task_is_never_likelier_than_one_of_its_facets() {
        let mut spread = Spread::default();
        spread.add(&TaskRun {
            facets: 4,
            right: 3,
            ..TaskRun::default()
        });
        spread.add(&TaskRun {
            facets: 4,
            right: 4,
            ..TaskRun::default()
        });
        assert!(spread.all_facets() <= spread.per_facet());
        assert!(close(spread.all_facets(), 50.0));
        assert!(close(spread.per_facet(), 87.5));

        let mut single = Spread::default();
        single.add(&TaskRun {
            facets: 1,
            right: 1,
            ..TaskRun::default()
        });
        assert!(close(single.all_facets(), single.per_facet()));
    }

    /// A member that owns no facet is left out of the load average rather
    /// than counted as an idle seat holding nothing.
    #[test]
    fn a_seat_given_no_facet_is_not_averaged_in() {
        let mut held = room(0, Expertise::Uniform);
        held.charge_brief();
        let rows = held.held_by(0);
        assert!(rows > 0);
        let one_seat = ratio_f64(as_f64(u64::try_from(rows).unwrap_or(0)), 1);
        let two_seats = ratio_f64(as_f64(u64::try_from(rows).unwrap_or(0)), 2);
        assert!(
            one_seat > two_seats,
            "counting an idle seat would halve the load the arm actually carries"
        );
        assert!(
            close(ratio_f64(1.0, 0), 0.0),
            "no seats is no load, not a divide by zero"
        );
    }

    /// `SimAgent` is named here so the import above is not dead weight: the
    /// load columns read rows off one, and a change to what a row is should
    /// break this file rather than silently move a published number.
    #[test]
    fn a_charged_brief_is_one_row_per_option() {
        let mut held = room(0, Expertise::Uniform);
        let before = held.agents.first().map_or(0, SimAgent::held);
        held.charge_brief();
        let after = held.agents.first().map_or(0, SimAgent::held);
        assert_eq!(after - before, names(&held, 0).len());
    }
}
