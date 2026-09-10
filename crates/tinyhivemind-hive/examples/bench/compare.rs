//! The simulated multi-arm comparison engine.
//!
//! `compare` is what `cargo run --example bench` does with no flags: it runs
//! every arm — the ladder, the matched-budget vote, the crate default, the
//! tuned policy and its delegation and check-arm variants, and the earned
//! directory the ladder is handed — over the same sample of rooms, and prints
//! the resulting table. Kept apart from `main.rs` because this is the one
//! function with the most to say about what the benchmark measures, and it
//! says most of it through the doc comments on [`Totals`]'s fields.

use std::time::Instant;

use tinyhivemind_hive::{
    Directory, DirectoryPolicy, EpisodePolicy, Sequence, directory, trace::Trace,
};

use crate::TASK;
use crate::arms;
use crate::cli::Options;
use crate::metrics::{
    Aggregate, arm_header, arm_row, detail_header, detail_row, json_line, library_header,
    library_row, paired_against, paired_diff_line,
};
use crate::parallel;
use crate::policy::{
    blind_wide_policy, default_policy, deferring_policy, evidential_policy,
    knowing_deferring_policy, knowing_policy, refuting_policy, widened_policy,
};
use crate::rng::mix;
use crate::run::{
    AsideMode, run_episode, run_episode_checking, run_episode_exchanging_with, run_episode_with,
};
use crate::sim::{CheckStyle, Room, SPECIALIST_COST_UNIT};

/// Run every arm over the same rooms and print the comparison.
pub(crate) fn compare(options: &Options, rooms: &[Room]) -> Result<(), String> {
    let tuned = options.policy;
    println!("{}\n", options.cost_model.header());
    println!(
        "rooms {}  agents {}  options {}  eval noise ±{}\n\
         tuned policy: budget {}  quorum {}  blind {}  dominance {}  repetition {}\n",
        rooms.len(),
        options.agents,
        options.topics,
        options.noise,
        tuned.turn_budget,
        tuned.quorum.threshold,
        if tuned.blind_round { "yes" } else { "no" },
        tuned.dominance_cap,
        tuned.repetition_cap,
    );

    let (totals, wall) = run_arms(options, rooms)?;
    let arms: [(&str, &Aggregate); 24] = [
        ("ladder", &totals.ladder),
        ("vote", &totals.vote),
        ("hive", &totals.hive_default),
        ("hive+", &totals.hive_tuned),
        ("hive+ref", &totals.hive_refuting),
        ("hive+ev", &totals.hive_evidential),
        // Appended rather than interleaved: the six rows above are the
        // published table, and the paired-bootstrap seed below is derived
        // from an arm's index in this list.
        ("hive+dir", &totals.hive_knowing),
        ("hive+defer", &totals.hive_deferring),
        ("hive+dir+defer", &totals.hive_both),
        ("ladder+dir", &totals.ladder_directed),
        ("hive+aside", &totals.hive_aside),
        ("hive+ask", &totals.hive_ask),
        ("hive+aside!", &totals.hive_aside_informed),
        ("hive+fact", &totals.hive_aside_fact),
        ("hive+mute", &totals.hive_aside_mute),
        ("hive+along", &totals.hive_aside_alongside),
        ("hive+share", &totals.hive_aside_exchange),
        ("hive+hush", &totals.hive_aside_hush),
        ("hive+rounds", &totals.hive_exchange_rounds),
        ("hive+quiet", &totals.hive_exchange_quiet),
        ("hive+fact°", &totals.hive_aside_offfloor),
        ("hive+pooled", &totals.hive_pooled),
        ("hive+wide", &totals.hive_wide),
        ("hive+blind", &totals.hive_blind_wide),
    ];

    if options.json {
        for (name, arm) in arms {
            println!("{}", json_line(name, arm));
        }
        if options.cost {
            println!("{}", json_line("all-reasoning", &totals.all_reasoning));
        }
    }

    println!("{}", arm_header());
    for (name, arm) in arms {
        println!("{}", arm_row(name, arm));
    }

    // The library's own cost, under a heading that says so. It used to sit in
    // the table above, where `ns/step 0` and `episodes/s inf` on the `vote`
    // row read as "this arm is free" rather than "this arm never calls the
    // library" -- which is what they mean.
    println!("\nwhat the library itself costs, with every agent's time excluded");
    println!("{}", library_header());
    for (name, arm) in arms {
        println!("{}", library_row(name, arm));
    }

    println!("\n{}", detail_header());
    for (name, arm) in arms {
        println!("{}", detail_row(name, arm));
    }
    for (index, (name, arm)) in arms.iter().filter(|(name, _)| *name != "vote").enumerate() {
        let seed = mix(options.seed, 0xB007_57AA_u64.wrapping_add(index as u64));
        if let Some(line) = paired_diff_line(name, arm, &totals.vote, seed, 2000) {
            println!("{line}");
        }
    }

    check_arm_diffs(options, &totals);

    if options.cost {
        cost_table(&[
            ("vote", &totals.vote),
            ("ladder", &totals.ladder),
            ("ladder+dir", &totals.ladder_directed),
            ("hive+", &totals.hive_tuned),
            ("hive+dir+defer", &totals.hive_both),
            ("all-reasoning", &totals.all_reasoning),
        ]);
    }

    endings(&totals);
    println!(
        "library time {:.1} ms over {} steps ({:.0} ns/step, {:.0} episodes/s)",
        totals.hive_tuned.library_time.as_secs_f64() * 1_000.0,
        totals.hive_tuned.step_calls,
        totals.hive_tuned.nanos_per_step(),
        totals.hive_tuned.episodes_per_second(),
    );
    println!(
        "wall clock {:.1} ms for {} rooms across every arm",
        wall.as_secs_f64() * 1_000.0,
        rooms.len(),
    );
    Ok(())
}

/// Every arm's running totals over one sample of rooms.
#[derive(Default)]
struct Totals {
    /// A deliberation at the crate's own default policy.
    hive_default: Aggregate,
    /// The same at the tuned policy the sweep picked.
    hive_tuned: Aggregate,
    /// The tuned policy with `refutation_cap` on.
    hive_refuting: Aggregate,
    /// The same, plus `require_evidential`.
    hive_evidential: Aggregate,
    /// The tuned policy with the folded directory on.
    hive_knowing: Aggregate,
    /// The tuned policy with `!defer` bounded, and no directory.
    hive_deferring: Aggregate,
    /// The tuned policy, with a member that cannot separate its two best
    /// options spending a turn asking one peer — privately.
    hive_aside: Aggregate,
    /// The identical exchange, in the open. The control that isolates
    /// privacy from asking: same turns, same words, every member reads it.
    hive_ask: Aggregate,
    /// The private exchange again, aimed at whoever the room has heard ground
    /// the option rather than at whoever spoke first.
    hive_aside_informed: Aggregate,
    /// The aimed private exchange, carrying the fact that rules an option out
    /// rather than a reading of it to be averaged.
    hive_aside_fact: Aggregate,
    /// The same exchange on the same turns, with the answer discarded.
    hive_aside_mute: Aggregate,
    /// The aimed, fact-carrying exchange again, riding alongside each
    /// member's floor move rather than replacing one: one turn, two rows, and
    /// the room charged for the first only.
    hive_aside_alongside: Aggregate,
    /// The free row spent continuously: a contact on every turn, carrying
    /// every reading its author holds. Bounded by `--aside-cap` peers.
    hive_aside_exchange: Aggregate,
    /// The same continuous exchange, run **off the floor** in rounds between
    /// turns: nobody takes the floor for it, so its volume is set by the
    /// policy rather than by how many turns the room happens to take. Its
    /// price is the `calls/ep` column.
    hive_exchange_rounds: Aggregate,
    /// `hive+share` with every answer discarded: the same rows riding
    /// alongside the same turns, transferring nothing. Alongside rows land
    /// unevenly — only on turns whose author wanted one — so unlike a round
    /// they do not shift every gap by a constant, and this is what says
    /// whether that unevenness moves the room on its own.
    hive_aside_hush: Aggregate,
    /// The off-floor rounds again with every answer discarded: same rows,
    /// same sequence numbers consumed, nothing transferred. What it moves is
    /// what writing the rows does to salience decay, not what they said.
    hive_exchange_quiet: Aggregate,
    /// The aimed, fact-carrying exchange again, held off the floor: the same
    /// bounded number of contacts, spending no turn the room could have
    /// deliberated with.
    hive_aside_offfloor: Aggregate,
    /// The ceiling: every reading and every fact already in every member's
    /// hands before the episode opens, at no turn cost. Nothing a protocol
    /// could do beats it.
    hive_pooled: Aggregate,
    /// The tuned policy run in **concurrent rounds** rather than one turn at a
    /// time. The arm ADR 0014 has to earn its place against: what should move
    /// is `rounds/ep`, and `correct %` says what the depth cost.
    hive_wide: Aggregate,
    /// The same, widened **only while the room is blind**. The free half of
    /// concurrency on its own: a blind member cannot read a peer's row whether
    /// or not it runs concurrently, so this should score what `hive+` scores
    /// and wait fewer times.
    hive_blind_wide: Aggregate,
    /// Both delegation mechanisms at once.
    hive_both: Aggregate,
    /// The tuned policy in a room that puts every seat on the expensive
    /// tier. Only filled under `--cost-tiers`.
    all_reasoning: Aggregate,
    /// The matched-budget independent poll.
    vote: Aggregate,
    /// One responder off the real ladder, chosen without information.
    ladder: Aggregate,
    /// The same ladder, given a directory the room earned.
    ladder_directed: Aggregate,
}

impl Totals {
    /// An empty set of totals, every arm priced against `model`.
    ///
    /// The whole table reports at one point of the cost model, which is what
    /// lets the header describe every row under it and what
    /// [`Aggregate::merge`]'s debug assertion holds the parallel fold to.
    fn priced_at(model: crate::cost::CostModel) -> Self {
        let mut totals = Self::default();
        for arm in totals.arms_mut() {
            arm.model = model;
        }
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
    fn merge(&mut self, other: &Self) {
        for (mine, theirs) in self.arms_mut().into_iter().zip(other.arms()) {
            mine.merge(theirs);
        }
        self.ladder_directed.merge(&other.ladder_directed);
    }
}

/// Run every arm over the same rooms, and say how long the whole sample took.
///
/// # Errors
///
/// Returns the library's own error text from any arm.
/// Run every arm that turns on a pairwise check, over one room.
///
/// Split out of [`run_arms`] because there are now seven of them and they
/// form one experiment: three that vary who reads the answer and where the
/// question is aimed, one matched-turn control that throws the answer away,
/// one that holds the same exchange off the floor, and one free ceiling. Read
/// together they separate what an exchange is worth from what its turns cost;
/// read one at a time they do not.
///
/// # Errors
///
/// Returns the library's own error text from any arm.
/// Print the check arms against the room they modify, rather than against the
/// poll.
///
/// Seeded off a tag of their own so the published bootstraps keep their
/// streams.
fn check_arm_diffs(options: &Options, totals: &Totals) {
    for (index, (name, arm)) in [
        ("hive+aside", &totals.hive_aside),
        ("hive+ask", &totals.hive_ask),
        ("hive+aside!", &totals.hive_aside_informed),
        ("hive+fact", &totals.hive_aside_fact),
        ("hive+mute", &totals.hive_aside_mute),
        ("hive+along", &totals.hive_aside_alongside),
        ("hive+share", &totals.hive_aside_exchange),
        ("hive+hush", &totals.hive_aside_hush),
        ("hive+rounds", &totals.hive_exchange_rounds),
        ("hive+quiet", &totals.hive_exchange_quiet),
        ("hive+fact°", &totals.hive_aside_offfloor),
        ("hive+pooled", &totals.hive_pooled),
        ("hive+wide", &totals.hive_wide),
        ("hive+blind", &totals.hive_blind_wide),
    ]
    .iter()
    .enumerate()
    {
        let seed = mix(options.seed, 0xA51D_E000_u64.wrapping_add(index as u64));
        if let Some(line) = paired_against(name, "hive+", arm, &totals.hive_tuned, seed, 2000) {
            println!("{line}");
        }
    }
    if let Some(line) = paired_against(
        "hive+aside",
        "hive+ask",
        &totals.hive_aside,
        &totals.hive_ask,
        mix(options.seed, 0xA51D_E100),
        2000,
    ) {
        println!("{line}");
    }
}

/// Run every check-and-exchange arm over one room and fold each into
/// `totals`.
///
/// Split out of [`run_arms`] because there are now seven of them and they
/// form one experiment: three that vary who reads the answer and where the
/// question is aimed, one matched-turn control that throws the answer away,
/// one that holds the same exchange off the floor, and one free ceiling. Read
/// together they separate what an exchange is worth from what its turns cost;
/// read one at a time they do not.
///
/// # Errors
///
/// Returns the library's own error text from any arm.
fn run_check_arms(
    options: &Options,
    room: &Room,
    tuned: &EpisodePolicy,
    totals: &mut Totals,
) -> Result<(), String> {
    let check = |mode: AsideMode, style: CheckStyle| {
        run_episode_checking(room, tuned, TASK, false, 0, mode, options.aside_cap, style)
    };
    // The pair that isolates privacy. Both spend a turn asking and a turn
    // answering; they differ in who may read the answer, and in nothing
    // else. `--aside-cap 0` leaves both bit-identical to `hive+`.
    totals
        .hive_aside
        .add(&check(AsideMode::Private, CheckStyle::PLAIN)?);
    totals
        .hive_ask
        .add(&check(AsideMode::Public, CheckStyle::PLAIN)?);
    // The informed variant: the check goes to whoever the room has heard
    // ground this option. It exists to close the obvious objection to a
    // negative result — that the question went to the wrong peer.
    totals
        .hive_aside_informed
        .add(&check(AsideMode::Private, CheckStyle::AIMED)?);
    // The same aimed exchange, carrying the fact rather than a number.
    // This is the arm that asks whether an aside is worth anything once
    // it carries what the room's public grammar has always carried.
    totals
        .hive_aside_fact
        .add(&check(AsideMode::Private, CheckStyle::FACT)?);
    // The matched-turn control the comparison always needed: the same
    // words on the same turns, with the answer thrown away. What it
    // loses against `hive+` is what the turns cost; what any arm above
    // gains over it is what the answer is worth.
    totals
        .hive_aside_mute
        .add(&check(AsideMode::Private, CheckStyle::MUTE)?);
    // The same aimed, fact-carrying exchange, riding alongside each member's
    // floor move instead of replacing one: one turn, two rows, the second of
    // which the episode cannot see. Same words and same targeting as
    // `hive+fact`; the room simply is not charged for it.
    totals
        .hive_aside_alongside
        .add(&check(AsideMode::Alongside, CheckStyle::ALONGSIDE)?);
    // The same free row, spent continuously: a contact on every turn carrying
    // every reading its author holds, bounded by how many distinct peers
    // `--aside-cap` allows. This is the arm a charged row could never afford.
    totals
        .hive_aside_exchange
        .add(&check(AsideMode::Alongside, CheckStyle::EXCHANGE)?);
    // `hive+share` saying nothing: the control for an *alongside* row, whose
    // sequence lands unevenly rather than once per turn.
    totals
        .hive_aside_hush
        .add(&check(AsideMode::Alongside, CheckStyle::QUIET)?);
    // The same continuous exchange, run off the floor: one round between every
    // pair of turns, bounded by `ExchangePolicy` rather than by the number of
    // turns the room takes. Priced in `calls/ep`.
    totals
        .hive_exchange_rounds
        .add(&run_episode_exchanging_with(
            room,
            tuned,
            TASK,
            false,
            options.exchange_cap,
            CheckStyle::EXCHANGE,
        )?);
    // The same rounds writing the same rows, with every answer discarded. The
    // difference between this and `hive+rounds` is what the exchange said; what
    // this arm moves on its own is what writing private rows does to a decay
    // that reads recency off raw sequence distance.
    totals.hive_exchange_quiet.add(&run_episode_exchanging_with(
        room,
        tuned,
        TASK,
        false,
        options.exchange_cap,
        CheckStyle::QUIET,
    )?);
    // The same bounded exchange, held off the floor entirely and given oracle
    // targeting. It bounds what the alongside arm above could reach.
    totals.hive_aside_offfloor.add(&run_episode(
        &room.pre_checked(options.aside_cap, true),
        tuned,
        TASK,
        false,
    )?);
    // `Room::pooled` is not gated by `cap` -- unlike the check arms above, it
    // has no notion of a bounded number of contacts. But `--aside-cap 0` is
    // documented and used as the kill switch that leaves every aside arm
    // bit-identical to `hive+`, `hive+pooled` included, so honor it here by
    // skipping the pool rather than silently pooling regardless of the cap.
    let ceiling = if options.aside_cap == 0 {
        room.clone()
    } else {
        room.pooled()
    };
    totals
        .hive_pooled
        .add(&run_episode(&ceiling, tuned, TASK, false)?);
    // The concurrency arm: the tuned policy, run in rounds rather than one
    // turn at a time. Same rooms, same budget, same everything else -- what
    // moves is `rounds/ep`, and whether `correct %` pays for it.
    totals.hive_wide.add(&run_episode(
        room,
        &widened_policy(tuned, options.round_width),
        TASK,
        false,
    )?);
    totals.hive_blind_wide.add(&run_episode(
        room,
        &blind_wide_policy(tuned, options.round_width),
        TASK,
        false,
    )?);
    Ok(())
}

/// Run every arm over the same rooms, and say how long the whole sample
/// took.
///
/// # Errors
///
/// Returns the library's own error text from any arm.
fn run_arms(options: &Options, rooms: &[Room]) -> Result<(Totals, std::time::Duration), String> {
    let tuned = options.policy;
    let default = default_policy();
    let refuting = refuting_policy(&tuned);
    let evidential = evidential_policy(&tuned);
    let knowing = knowing_policy(&tuned);
    let deferring = deferring_policy(&tuned, options.defer_cap);
    let both = knowing_deferring_policy(&tuned, options.defer_cap);
    let wall = Instant::now();
    // Each room is decided into totals of its own, in a worker, and those are
    // merged here in room order. The body below is exactly the sequential
    // fold it replaced — what changed is who owns the `Totals` it folds into.
    // See `crate::parallel` for why the merge order is not a detail.
    // Indexed rather than bare, because two arms seed themselves off the
    // room's position in the sample and a worker cannot recover it from a
    // reference.
    let indexed: Vec<(usize, &Room)> = rooms.iter().enumerate().collect();
    let per_room: Vec<Totals> = parallel::map_in_order(&indexed, options.jobs, |(index, room)| {
        let (index, room) = (*index, *room);
        let mut totals = Totals::priced_at(options.cost_model);

        totals
            .hive_default
            .add(&run_episode(room, &default, TASK, false)?);
        totals
            .hive_tuned
            .add(&run_episode(room, &tuned, TASK, false)?);
        totals
            .hive_refuting
            .add(&run_episode(room, &refuting, TASK, false)?);
        totals
            .hive_evidential
            .add(&run_episode(room, &evidential, TASK, false)?);
        // The three delegation arms. Only the deferring two hand their members
        // a non-zero cap, so `hive+dir` differs from `hive+` in the policy
        // field alone and in nothing a participant does.
        totals
            .hive_knowing
            .add(&run_episode(room, &knowing, TASK, false)?);
        totals.hive_deferring.add(&run_episode_with(
            room,
            &deferring,
            TASK,
            false,
            options.defer_cap,
        )?);
        totals.hive_both.add(&run_episode_with(
            room,
            &both,
            TASK,
            false,
            options.defer_cap,
        )?);
        if options.cost {
            totals.all_reasoning.add(&run_episode(
                &room.at_cost(SPECIALIST_COST_UNIT),
                &tuned,
                TASK,
                false,
            )?);
        }
        run_check_arms(options, room, &tuned, &mut totals)?;
        let seed = mix(options.seed, u64::try_from(index).unwrap_or(0));
        totals.ladder.add_arm(&arms::run_ladder(room, seed)?);
        let earned = earn_directory(room, &tuned, options.history, mix(seed, 0x6869_7374))?;
        totals
            .ladder_directed
            .add_arm(&arms::run_ladder_directed(room, &earned, seed)?);
        // The control is given the whole budget, which is more turns than the
        // deliberation actually spends. It is the arm to beat, so it gets
        // every advantage.
        totals
            .vote
            .add_arm(&arms::run_vote(room, tuned.turn_budget));
        Ok(totals)
    })?;

    let mut totals = Totals::priced_at(options.cost_model);
    for chunk in &per_room {
        totals.merge(chunk);
    }
    Ok((totals, wall.elapsed()))
}

/// Print how each deliberating arm's episodes ended.
fn endings(totals: &Totals) {
    println!();
    for (name, arm) in [
        ("hive ", &totals.hive_default),
        ("hive+", &totals.hive_tuned),
        ("hive+ref", &totals.hive_refuting),
        ("hive+ev", &totals.hive_evidential),
        ("hive+dir+defer", &totals.hive_both),
    ] {
        println!(
            "{name} endings: converged {} · deadlocked {} · exhausted {} · idle {}",
            arm.converged, arm.deadlocked, arm.exhausted, arm.idle,
        );
    }
}

/// Earn a directory for one room by running `--history` prior episodes of
/// `hive+` on it and folding the whole record once.
///
/// The `ladder+dir` arm is not allowed to be handed the answer. A directory
/// invented by the harness, or read off `Room::experts`, would measure the
/// harness rather than the mechanism, so this earns one the only way the
/// library offers: it deliberates the same room several times and folds what
/// those transcripts recorded.
///
/// The several episodes are *concatenated with renumbered sequences* and
/// folded once, rather than folded separately and merged by summing weights.
/// Both were available; this one is chosen because it is the fold the library
/// actually defines. Summing weights across separate folds would double-count
/// the `WEIGHT_CEILING` clamp and would apply each episode's decay from its
/// own end, so a member's total would depend on how the history happened to
/// be cut into episodes. One fold over one renumbered record has one decay
/// origin and one clamp. It also means [`DirectoryPolicy::DEFAULT`]'s
/// `window` of 30 sequences applies to the *whole* history: past about four
/// episodes of a five-member room the earliest ones fall out of window, which
/// is why `--history 5` is not simply a stronger `--history 3`.
///
/// Each replay is [`Room::resampled`] — the same members and the same private
/// evaluations, with only the noncompliance draw reseeded — because the
/// simulated participants are otherwise deterministic and replaying a room
/// would produce the same transcript N times. `seed` is this room's own
/// stream, so two rooms do not share a resampling.
///
/// # Errors
///
/// Returns the library's own error text from an episode or from the fold.
fn earn_directory(
    room: &Room,
    tuned: &EpisodePolicy,
    history: u32,
    seed: u64,
) -> Result<Directory, String> {
    let mut record: Vec<Trace> = Vec::new();
    let mut offset = 0_u64;
    for episode in 0..history {
        let seed = mix(seed, u64::from(episode));
        let report = run_episode(&room.resampled(seed), tuned, TASK, false)?;
        let mut highest = 0_u64;
        for trace in report.traces {
            highest = highest.max(trace.sequence.0);
            record.push(shift(trace, offset));
        }
        offset = offset.saturating_add(highest);
    }
    let at = Sequence(offset);
    directory(&record, at, &DirectoryPolicy::DEFAULT, &[]).map_err(|error| error.to_string())
}

/// Move one trace, and every sequence it names, forward by `offset`.
///
/// A citation names a sequence, so renumbering a trace without renumbering
/// what it cites would silently break every credibility term in the fold —
/// the citation would land on whatever the earlier episode happened to have
/// at that number.
fn shift(trace: Trace, offset: u64) -> Trace {
    Trace {
        sequence: Sequence(trace.sequence.0.saturating_add(offset)),
        target: trace
            .target
            .map(|target| Sequence(target.0.saturating_add(offset))),
        cites: trace
            .cites
            .into_iter()
            .map(|cited| Sequence(cited.0.saturating_add(offset)))
            .collect(),
        ..trace
    }
}

/// Print what each arm spent, and what its right answers cost.
///
/// Only under `--cost-tiers`, where a specialist's turn is charged ten times
/// a lay member's and the question stops being "which arm is most accurate"
/// and becomes "which arm is most accurate per unit spent". `correct/kU` is
/// right answers per thousand cost units: an arm that buys two more points of
/// accuracy by putting every seat on the expensive tier should be visible
/// here as having bought them badly.
fn cost_table(arms: &[(&str, &Aggregate)]) {
    println!(
        "\n{:<15}{:>11}{:>10}{:>14}",
        "arm", "correct %", "cost/ep", "correct/kU",
    );
    for (name, totals) in arms {
        println!(
            "{:<15}{:>11.1}{:>10.2}{:>14.2}",
            name,
            totals.accuracy(),
            totals.cost_per_episode(),
            totals.accuracy_per_kilo_unit(),
        );
    }
}
