//! Driving one live desk or episode through a real backend.
//!
//! `live_episode` is what `--agent-cmd` and `--api-base` select: a single
//! room, seated over a CLI agent or the HTTP backend, deliberating either a
//! synthetic brief or a real `--scenario`. Split out of `main.rs` because it
//! is the largest single experiment the binary runs and because it is the
//! natural unit to compare against `live_swarm.rs`, which drives the same
//! backends across a federation of desks instead of one room.

use crate::TASK;
use crate::backend::{
    SeatUsage, backend_label, http_config, poll_backend, print_usage, seat_command, seat_model,
};
use crate::cli::Options;
use crate::http::HttpAgent;
use crate::live::{self, AgentPrompt, LiveAgent};
use crate::metrics;
use crate::policy::{quorum_threshold, turn_budget};
use crate::run::{Participant, drive};
use crate::scenario::{Scenario, ScenarioAgent};
use crate::sim::Room;
use tinyhivemind_hive::{
    EpisodePolicy, QuorumPolicy, Sequence,
    trace::{TopicId, Trace},
};

/// Print how the round used the member who actually held the deciding fact:
/// whether the scenario's `truth_expert` spoke at all, whether it spoke
/// *before* the room recorded its decision, and whether the winning commit's
/// citation chain reaches anything that member said.
///
/// Returns `(spoke before the commit, cited by the commit)` so the caller can
/// count both across `--repeat` rounds.
///
/// Mirrors what `run_episode` reports for a simulated room's own specialist
/// or decisive member; a live scenario has no `Room::experts` to read back,
/// so this reads `first_spoke` and the episode's own traces, which `drive`
/// already builds from turns it saw, against the id the scenario itself
/// names.
fn print_expert(scenario: &Scenario, report: &crate::run::EpisodeReport) -> (bool, bool) {
    let Some(expert) = &scenario.truth_expert else {
        return (false, false);
    };
    // Only an episode that recorded a decision has a commit boundary to be
    // before at all. `commit_at` alone is not enough to tell that: `Tally`
    // sets it the moment any turn runs in `Phase::Commit`, and a round can
    // still enter that phase, never emit a valid `!commit`, and exhaust its
    // budget with `decided` left `None` — that round must not be counted as
    // an early expert turn either.
    let commit_at = report
        .decided
        .is_some()
        .then_some(report.commit_at)
        .flatten();
    let spoke = report
        .first_spoke
        .iter()
        .find(|(id, _)| id == expert)
        .map(|(_, turn)| *turn);
    let before = spoke
        .zip(commit_at)
        .is_some_and(|(turn, commit)| turn < commit);
    match spoke {
        Some(turn) => println!(
            "hive   expert @{expert} first spoke at turn {turn} ({} the commit)",
            if before { "before" } else { "not before" },
        ),
        None => println!("hive   expert @{expert} never spoke"),
    }
    // Only an episode that actually recorded a decision has a winning commit
    // whose citation chain there is anything to walk.
    let cited = match &report.decided {
        Some(topic) => {
            let cited = commit_reaches(&report.traces, topic, expert);
            println!(
                "hive   the winning commit's citation chain {} @{expert}",
                if cited { "reaches" } else { "does not reach" },
            );
            cited
        }
        None => false,
    };
    println!("hive   {} turn(s) spent on !defer", report.defers);
    (before, cited)
}

/// Whether the `!commit` for `topic` cites, directly or through any chain of
/// citations, a message `expert` authored.
///
/// This is the question a scenario's `truth_expert` exists to make askable:
/// a room can reach the right answer while having ignored the member who
/// held the fact, and a room that reached it *through* that member has done
/// something the poll cannot. Walked breadth-first over the traces the
/// episode's own journal produced, with a visited set, so a citation cycle
/// terminates.
fn commit_reaches(traces: &[Trace], topic: &TopicId, expert: &str) -> bool {
    let Some(commit) = traces.iter().find(|trace| {
        trace.kind == tinyhivemind_hive::TraceKind::Commit && trace.topic.as_ref() == Some(topic)
    }) else {
        return false;
    };
    let mut frontier: Vec<Sequence> = commit.cites.clone();
    frontier.extend(commit.target);
    let mut seen: Vec<Sequence> = Vec::new();
    while let Some(at) = frontier.pop() {
        if seen.contains(&at) {
            continue;
        }
        seen.push(at);
        for trace in traces.iter().filter(|trace| trace.sequence == at) {
            if trace.agent_id() == Some(expert) {
                return true;
            }
            frontier.extend(trace.cites.iter().copied());
            frontier.extend(trace.target);
        }
    }
    false
}

/// Print who the scenario says the winning option was somebody's call, and
/// whether that member actually backed it.
///
/// `expert:` on an option is the scenario's own claim that one member's
/// judgment on that specific call is worth deferring to. It is never shown
/// to the room — telling the room who to trust per option would hand it the
/// answer — so it is only ever read back here, after the round has decided.
fn print_option_expert(
    scenario: &Scenario,
    decided: Option<&str>,
    report: &crate::run::EpisodeReport,
) {
    let Some(decided) = decided else {
        return;
    };
    let Some(expert) = scenario
        .options
        .iter()
        .find(|option| option.id == decided)
        .and_then(|option| option.expert.as_deref())
    else {
        return;
    };
    let backed = report.traces.iter().any(|trace| {
        matches!(
            trace.kind,
            tinyhivemind_hive::TraceKind::Propose | tinyhivemind_hive::TraceKind::Support
        ) && trace.topic.as_ref().map(TopicId::as_str) == Some(decided)
            && trace.agent_id() == Some(expert)
    });
    println!(
        "hive   #{decided} is @{expert}'s call — they {} it",
        if backed { "backed" } else { "never backed" },
    );
}

/// Build one participant for a scenario member, honoring per-seat overrides.
///
/// # Errors
///
/// Returns a message naming the missing command, key, or base.
fn seat_participant(
    options: &Options,
    agent: &ScenarioAgent,
    quorum: QuorumPolicy,
) -> Result<(Box<dyn Participant>, Option<SeatUsage>), String> {
    let private = Scenario::private_brief(agent);
    if options.api_base.is_some() {
        let config = http_config(options)?;
        let model = seat_model(options, agent).to_owned();
        let prompt = AgentPrompt::new(&agent.id, &agent.role, quorum, private);
        let http_agent = HttpAgent::new(prompt, config, model.clone());
        let usage = SeatUsage {
            agent_id: agent.id.clone(),
            model,
            tier: agent.tier.clone(),
            handle: http_agent.usage_handle(),
        };
        return Ok((Box::new(http_agent), Some(usage)));
    }
    let command = seat_command(options, agent)?;
    let live = LiveAgent::new(
        &agent.id,
        &agent.role,
        command,
        quorum,
        private,
        options.timeout,
    )
    .ok_or_else(|| format!("could not build agent from {command:?}"))?;
    Ok((Box::new(live), None))
}

/// Build one participant for the synthetic (no-scenario) room.
///
/// Per-seat overrides only make sense against a named scenario member, so the
/// synthetic room only ever runs the backend's default command or model.
///
/// # Errors
///
/// Returns a message naming the missing command, key, or base.
fn seat_synthetic(
    options: &Options,
    id: &str,
    role: &str,
    quorum: QuorumPolicy,
) -> Result<(Box<dyn Participant>, Option<SeatUsage>), String> {
    if options.api_base.is_some() {
        let config = http_config(options)?;
        let model = options.model.clone();
        let prompt = AgentPrompt::new(id, role, quorum, String::new());
        let http_agent = HttpAgent::new(prompt, config, model.clone());
        let usage = SeatUsage {
            agent_id: id.to_owned(),
            model,
            tier: None,
            handle: http_agent.usage_handle(),
        };
        return Ok((Box::new(http_agent), Some(usage)));
    }
    let command = options
        .agent
        .as_deref()
        .ok_or_else(|| "no --agent-cmd or --api-base given".to_owned())?;
    let live = LiveAgent::new(id, role, command, quorum, String::new(), options.timeout)
        .ok_or_else(|| format!("could not build agent from {command:?}"))?;
    Ok((Box::new(live), None))
}

/// Drive one episode through a real agent CLI or an HTTP backend.
///
/// Without `--scenario` the room deliberates over a synthetic brief, which
/// measures whether an agent can hold the trace grammar and nothing else.
/// With one it decides a real problem whose answer is recorded, and the
/// independent-vote control is run against the same agents so the deliberation
/// has something to be scored against.
pub(crate) fn live_episode(options: &Options) -> Result<(), String> {
    if let Some(directory) = &options.scenario_dir {
        return live_corpus(options, directory);
    }
    match &options.scenario {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .map_err(|error| format!("could not read {path}: {error}"))?;
            let scenario = Scenario::parse(&text)?;
            live_scenario(options, &scenario)
        }
        None => live_synthetic(options),
    }
}

/// Run every scenario in a directory, in name order.
///
/// One problem is one problem. A room that gets an SRE incident right has been
/// shown to hold the grammar on an SRE incident, and the shipped corpus was a
/// single genre for long enough that it was worth saying so out loud: this runs
/// the lot, so diversity is a measured axis rather than a claim about the
/// fixtures.
///
/// Scenarios are run in sorted file-name order so two runs of the same
/// directory report in the same order, and each is announced before it runs —
/// a live corpus takes minutes per scenario, and a caller watching it needs to
/// know which one is spending them.
///
/// # Errors
///
/// Returns a read or parse failure for the directory, for any entry in it, or
/// for any scenario. A scenario that fails to *read* or to *parse* stops the
/// run rather than being skipped: a silently ignored fixture is a corpus that
/// quietly shrinks, and a run that reports a corpus it did not finish is worse
/// than one that refuses.
fn live_corpus(options: &Options, directory: &str) -> Result<(), String> {
    // Every entry is resolved before any is filtered, and a failure to read
    // one stops the run. `read_dir` succeeding does not mean iterating it
    // will, and dropping the entries that fail is exactly the quietly
    // shrinking corpus this function's own contract refuses.
    let entries: Vec<std::fs::DirEntry> = std::fs::read_dir(directory)
        .map_err(|error| format!("could not read {directory}: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("could not read an entry of {directory}: {error}"))?;
    let mut paths: Vec<std::path::PathBuf> = entries
        .into_iter()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|kind| kind == "txt"))
        .collect();
    paths.sort();
    if paths.is_empty() {
        return Err(format!("no .txt scenarios in {directory}"));
    }

    println!("corpus {directory}: {} scenarios\n", paths.len());
    for path in &paths {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        );
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("could not read {}: {error}", path.display()))?;
        let scenario = Scenario::parse(&text)?;
        println!("── {name} ──");
        live_scenario(options, &scenario)?;
        println!();
    }
    Ok(())
}

/// Deliberate a real problem, then poll the same agents independently.
///
/// Both arms are run `--repeat` times, because a live room is sampled rather
/// than computed and one episode is an anecdote. The trace is printed for the
/// first round only; the rest are counted.
fn live_scenario(options: &Options, scenario: &Scenario) -> Result<(), String> {
    let ids = scenario.member_ids();
    // The room size comes from the scenario, so the quorum and budget have to
    // follow it rather than the `--agents` default.
    let policy = EpisodePolicy {
        turn_budget: turn_budget(ids.len()),
        quorum: QuorumPolicy {
            threshold: quorum_threshold(ids.len()),
            ..options.policy.quorum
        },
        ..options.policy
    };

    println!(
        "driving {} episode(s) through {}\n\
         {} members, budget {}, quorum {}\n\nThe brief every member sees:\n{}",
        options.repeat,
        backend_label(options),
        ids.len(),
        policy.turn_budget,
        policy.quorum.threshold,
        scenario.brief(),
    );

    let mut hive_correct = 0_u32;
    let mut hive_decided = 0_u32;
    let mut vote_correct = 0_u32;
    let mut turns_total = 0_u32;
    let mut expert_early = 0_u32;
    let mut expert_cited = 0_u32;
    let mut defers_total = 0_u32;
    for round in 0..options.repeat {
        let outcome = live_round(options, scenario, &policy, &ids, round == 0)?;
        if outcome.expert_early {
            expert_early = expert_early.saturating_add(1);
        }
        if outcome.expert_cited {
            expert_cited = expert_cited.saturating_add(1);
        }
        defers_total = defers_total.saturating_add(outcome.defers);
        if outcome.decided.is_some() {
            hive_decided = hive_decided.saturating_add(1);
        }
        if outcome.decided.as_deref() == Some(scenario.truth.as_str()) {
            hive_correct = hive_correct.saturating_add(1);
        }
        if outcome.voted.as_deref() == Some(scenario.truth.as_str()) {
            vote_correct = vote_correct.saturating_add(1);
        }
        turns_total = turns_total.saturating_add(outcome.turns);
    }

    println!(
        "over {} round(s), answer #{}:\n\
         hive   {} correct, {} decided, {:.1} turns per episode\n\
         hive   expert spoke before the commit in {} round(s), cited by the commit in {}\n\
         hive   {} turn(s) spent on !defer across every round\n\
         vote   {} correct",
        options.repeat,
        scenario.truth,
        hive_correct,
        hive_decided,
        metrics::ratio(u64::from(turns_total), u64::from(options.repeat)),
        expert_early,
        expert_cited,
        defers_total,
        vote_correct,
    );
    Ok(())
}

/// What one live round of both arms decided.
struct RoundOutcome {
    /// The topic the deliberation settled on.
    decided: Option<String>,
    /// The topic the independent poll returned, if it was not tied.
    voted: Option<String>,
    /// Turns the deliberation took.
    turns: u32,
    /// Whether the scenario's `truth_expert` spoke before the commit.
    expert_early: bool,
    /// Whether the winning commit's citation chain reaches that member.
    expert_cited: bool,
    /// Turns the round spent on `!defer`.
    defers: u32,
}

/// Run one deliberation and one independent poll over the same room.
fn live_round(
    options: &Options,
    scenario: &Scenario,
    policy: &EpisodePolicy,
    ids: &[&str],
    keep_trace: bool,
) -> Result<RoundOutcome, String> {
    let mut agents: Vec<Box<dyn Participant>> = Vec::new();
    let mut usage_seats: Vec<SeatUsage> = Vec::new();
    for agent in &scenario.agents {
        let (participant, usage) = seat_participant(options, agent, policy.quorum)?;
        agents.push(participant);
        if let Some(usage) = usage {
            usage_seats.push(usage);
        }
    }
    if agents.len() != ids.len() {
        return Err("could not seat every scenario member".to_owned());
    }
    // A plain loop rather than `.iter_mut().map(...).collect()`: collecting
    // trait-object references straight out of a `Vec<Box<dyn Participant>>`
    // makes dropck conservatively extend `agents`' borrow to the end of the
    // function, since it cannot see that the reference never outlives the
    // `drive` call below.
    let mut participants: Vec<&mut dyn Participant> = Vec::new();
    for agent in &mut agents {
        participants.push(agent.as_mut());
    }

    let wall = std::time::Instant::now();
    let report = drive(
        ids,
        &mut participants,
        policy,
        &scenario.brief(),
        keep_trace,
    )?;
    let wall = wall.elapsed();
    for line in &report.trace {
        println!("{line}");
    }
    let decided = report.decided.as_ref().map(TopicId::as_str);
    println!(
        "\nhive   ended {} on {} after {} turns in {:.0} s — {}",
        report.ending.label(),
        decided.map_or_else(|| "nothing".to_owned(), |topic| format!("#{topic}")),
        report.turns,
        wall.as_secs_f64(),
        verdict(decided, &scenario.truth),
    );
    let (expert_early, expert_cited) = print_expert(scenario, &report);
    print_option_expert(scenario, decided, &report);
    print_usage(options, &usage_seats);

    let backend = poll_backend(options)?;
    let picks = live::poll(options, scenario, &backend)?;
    for (id, pick) in &picks {
        println!("vote   {id:>10} alone: #{pick}");
    }
    let voted = plurality(&picks);
    match &voted {
        Some(topic) => println!(
            "vote   plurality #{topic} of {} — {}",
            picks.len(),
            verdict(Some(topic.as_str()), &scenario.truth),
        ),
        None => println!("vote   tied, no plurality — no answer"),
    }
    println!();

    Ok(RoundOutcome {
        decided: decided.map(str::to_owned),
        voted,
        turns: report.turns,
        expert_early,
        expert_cited,
        defers: report.defers,
    })
}

/// The single option with the most votes, or `None` when the poll is tied.
///
/// A tie is not a decision and must not be reported as one. Resolving it by
/// the order the votes happened to arrive would hand the vote control a win it
/// did not earn, which is exactly the kind of quiet thumb on the scale a
/// benchmark exists to avoid.
pub(crate) fn plurality(picks: &[(String, String)]) -> Option<String> {
    let mut tally: Vec<(&str, u32)> = Vec::new();
    for (_, pick) in picks {
        match tally.iter_mut().find(|(topic, _)| *topic == pick.as_str()) {
            Some(entry) => entry.1 = entry.1.saturating_add(1),
            None => tally.push((pick.as_str(), 1)),
        }
    }
    let most = tally.iter().map(|(_, count)| *count).max()?;
    let mut leaders = tally.iter().filter(|(_, count)| *count == most);
    let leader = leaders.next()?;
    if leaders.next().is_some() {
        return None;
    }
    Some(leader.0.to_owned())
}

/// Whether an arm landed on the recorded answer.
pub(crate) fn verdict(decided: Option<&str>, truth: &str) -> &'static str {
    match decided {
        Some(topic) if topic == truth => "correct",
        Some(_) => "wrong",
        None => "no answer",
    }
}

/// Deliberate the synthetic brief, which has no recorded answer.
fn live_synthetic(options: &Options) -> Result<(), String> {
    let room = Room::generate(options.seed, options.agents, options.topics, options.noise);
    let ids = room.member_ids();
    let roles = [
        "planner, who proposes concrete options",
        "critic, who looks for the weakness in a proposal",
        "archivist, who supplies precedent and evidence",
        "scout, who looks for the option nobody has raised",
        "auditor, who checks a decision against the constraints",
    ];
    let mut agents: Vec<Box<dyn Participant>> = Vec::new();
    let mut usage_seats: Vec<SeatUsage> = Vec::new();
    for (index, id) in ids.iter().enumerate() {
        let role = roles.get(index).copied().unwrap_or("teammate");
        let (participant, usage) = seat_synthetic(options, id, role, options.policy.quorum)?;
        agents.push(participant);
        if let Some(usage) = usage {
            usage_seats.push(usage);
        }
    }
    if agents.len() != ids.len() {
        return Err("could not seat every synthetic member".to_owned());
    }
    // A plain loop rather than `.iter_mut().map(...).collect()`: collecting
    // trait-object references straight out of a `Vec<Box<dyn Participant>>`
    // makes dropck conservatively extend `agents`' borrow to the end of the
    // function, since it cannot see that the reference never outlives the
    // `drive` call below.
    let mut participants: Vec<&mut dyn Participant> = Vec::new();
    for agent in &mut agents {
        participants.push(agent.as_mut());
    }

    println!("driving one episode through {}\n", backend_label(options));
    let report = drive(&ids, &mut participants, &options.policy, TASK, true)?;
    for line in &report.trace {
        println!("{line}");
    }
    println!(
        "\nended {} on {} after {} turns, {:.0} ns/step of library time",
        report.ending.label(),
        report
            .decided
            .as_ref()
            .map_or_else(|| "nothing".to_owned(), |topic| format!("#{topic}")),
        report.turns,
        metrics::ratio(
            u64::try_from(report.library_time.as_nanos()).unwrap_or(u64::MAX),
            u64::from(report.step_calls),
        ),
    );
    print_usage(options, &usage_seats);
    Ok(())
}
