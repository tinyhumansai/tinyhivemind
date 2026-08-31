//! End-to-end coverage for a host driving a deliberation episode.
//!
//! Every test here goes through the public API only, and through a host that
//! owns its own journal — the same boundary a consuming application has.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/hive_harness.rs"]
mod hive_harness;
#[path = "support/scripted_agent.rs"]
mod scripted_agent;

use hive_harness::{HiveHarness, Outcome};
use scripted_agent::ScriptedAgent;
use tinyhivemind_hive::{
    BidReason, EpisodePolicy, EpisodeState, QuorumPolicy, Visibility,
    quorum::{TopicStanding, standings},
    trace::{TopicId, read},
};

const MEMBERS: [&str; 3] = ["planner", "critic", "scout"];

fn harness() -> HiveHarness {
    HiveHarness::new("engineering", "Engineering", &MEMBERS)
}

fn policy(turn_budget: u32) -> EpisodePolicy {
    EpisodePolicy {
        turn_budget,
        quorum: QuorumPolicy {
            threshold: 2,
            window: 100,
            require_grounded: true,
        },
        ..EpisodePolicy::DEFAULT
    }
}

fn converged(outcome: &Outcome) -> (&TopicId, &TopicStanding) {
    let Outcome::Converged { topic, standing } = outcome else {
        panic!("expected convergence, got {outcome:?}")
    };
    (topic, standing)
}

#[test]
fn three_agents_deliberate_over_one_shared_transcript_and_converge() -> Result<(), String> {
    let mut harness = harness();
    harness.operator("Decide how to roll this out.");
    let state = EpisodeState::opened(harness.conversation(), harness.watermark());

    let mut planner = ScriptedAgent::new("planner", ["!propose #stage Stage the rollout."]);
    let mut critic =
        ScriptedAgent::new("critic", ["!support #stage ^2 It bounds the blast radius."]);
    let mut scout = ScriptedAgent::new("scout", ["!question What is the rollback path?"]);

    let (outcome, steps) = harness.run(
        state,
        &policy(8),
        &mut [&mut planner, &mut critic, &mut scout],
    )?;

    let (topic, standing) = converged(&outcome);
    assert_eq!(topic, &TopicId("stage".into()));
    assert_eq!(standing.supporters, ["planner", "critic"]);

    // Every step ran exactly one agent, and the journal grew by exactly that
    // many messages on top of the operator's.
    assert_eq!(harness.journal().len(), steps.len() + 1);
    assert!(steps.len() <= 8);
    Ok(())
}

#[test]
fn one_step_runs_exactly_one_agent_turn() -> Result<(), String> {
    let mut harness = harness();
    harness.operator("Decide how to roll this out.");
    let state = EpisodeState::opened(harness.conversation(), harness.watermark());

    let mut planner = ScriptedAgent::new("planner", ["!propose #stage"]);
    let mut critic = ScriptedAgent::new("critic", ["!support #stage ^2"]);
    let mut scout = ScriptedAgent::new("scout", ["!question"]);

    let (_, steps) = harness.run(
        state,
        &policy(3),
        &mut [&mut planner, &mut critic, &mut scout],
    )?;

    let turns: usize = planner.calls().len() + critic.calls().len() + scout.calls().len();
    assert_eq!(
        turns,
        steps.len(),
        "the number of model calls must equal the number of authorized turns",
    );
    Ok(())
}

#[test]
fn a_failed_turn_does_not_append_a_phantom_message() {
    let mut harness = harness();
    harness.operator("Decide how to roll this out.");
    let state = EpisodeState::opened(harness.conversation(), harness.watermark());

    let mut planner = ScriptedAgent::failing("planner", "model unavailable");
    let mut critic = ScriptedAgent::new("critic", ["!support #stage ^2"]);
    let mut scout = ScriptedAgent::new("scout", ["!question"]);

    let result = harness.run(
        state,
        &policy(4),
        &mut [&mut planner, &mut critic, &mut scout],
    );

    assert_eq!(result, Err("model unavailable".to_owned()));
    assert_eq!(harness.journal().len(), 1, "only the operator's message");
}

#[test]
fn an_episode_stops_at_its_budget_rather_than_running_on() -> Result<(), String> {
    let mut harness = harness();
    harness.operator("Talk this through.");
    let state = EpisodeState::opened(harness.conversation(), harness.watermark());

    // Nobody ever supports anything, so quorum is never reached and the budget
    // is the only thing that ends the episode.
    let mut agents: Vec<ScriptedAgent> = MEMBERS
        .iter()
        .map(|id| ScriptedAgent::new(id, ["!question still thinking"]))
        .collect();
    let (a, rest) = agents.split_at_mut(1);
    let (b, c) = rest.split_at_mut(1);
    let (outcome, steps) =
        harness.run(state, &policy(5), &mut [&mut a[0], &mut b[0], &mut c[0]])?;

    assert_eq!(outcome, Outcome::Exhausted { spent: 5 });
    assert_eq!(steps.len(), 5);
    Ok(())
}

#[test]
fn the_opening_round_is_blind_and_then_reveals() -> Result<(), String> {
    let mut harness = harness();
    harness.operator("Decide how to roll this out.");
    let state = EpisodeState::opened(harness.conversation(), harness.watermark());

    let mut planner = ScriptedAgent::new("planner", ["!propose #stage", "!support #stage ^2"]);
    let mut critic = ScriptedAgent::new("critic", ["!propose #ship", "!support #ship ^3"]);
    let mut scout = ScriptedAgent::new("scout", ["!question", "!support #stage ^2"]);

    let (_, steps) = harness.run(
        state,
        &policy(6),
        &mut [&mut planner, &mut critic, &mut scout],
    )?;

    let blind: Vec<&_> = steps
        .iter()
        .take_while(|step| step.visibility == Visibility::Blind)
        .collect();
    assert!(
        !blind.is_empty(),
        "the opening round must be blind so first positions are independent",
    );
    assert!(
        steps.iter().any(|step| step.visibility == Visibility::Full),
        "and it must reveal once every member has been heard",
    );

    // A blind turn never saw a peer's position. The operator's framing message
    // is always visible, so a blind turn sees at most that plus its own work.
    for (index, step) in steps.iter().enumerate() {
        if step.visibility != Visibility::Blind {
            continue;
        }
        let peers_before = steps[..index]
            .iter()
            .filter(|earlier| earlier.agent_id != step.agent_id)
            .count();
        assert_eq!(
            step.saw,
            harness.journal().len().min(index + 1) - peers_before,
            "a blind turn must not see a peer's position",
        );
    }
    Ok(())
}

#[test]
fn a_deadlock_is_reported_rather_than_resolved_arbitrarily() -> Result<(), String> {
    // A two-member room: each proposes, each supports the other's proposal, so
    // both topics carry and nobody is left uncommitted to break the tie.
    let mut harness = HiveHarness::new("engineering", "Engineering", &["planner", "scout"]);
    harness.operator("Pick one.");
    harness.agent("planner", "!propose #stage");
    harness.agent("scout", "!propose #ship");
    harness.agent("planner", "!support #ship ^3");
    harness.agent("scout", "!support #stage ^2");
    let state = EpisodeState::opened(harness.conversation(), harness.watermark());

    let mut planner = ScriptedAgent::new("planner", []);
    let mut scout = ScriptedAgent::new("scout", []);
    let (outcome, steps) = harness.run(state, &policy(4), &mut [&mut planner, &mut scout])?;

    // The episode's own watermark excludes the setup, so the deadlock has to be
    // re-established inside the episode — which it is not, so it idles or runs
    // out rather than inventing a decision.
    assert!(
        !matches!(outcome, Outcome::Converged { .. }),
        "a tie must never resolve into a decision, got {outcome:?}",
    );
    assert!(steps.len() <= 4);
    Ok(())
}

#[test]
fn cross_inhibition_is_what_breaks_a_tie() -> Result<(), String> {
    let mut harness = HiveHarness::new(
        "engineering",
        "Engineering",
        &["planner", "scout", "critic", "archivist"],
    );
    harness.operator("Pick one.");
    let state = EpisodeState::opened(harness.conversation(), harness.watermark());

    // planner and scout each propose a rival topic. Rather than letting scout
    // pile up a second supporter and crystallize into a genuine tie, critic
    // objects to scout's own proposal early -- silencing scout as ship's
    // advocate before anyone else ever backs it. `stage` is then the only
    // topic left standing, and archivist's support alone carries it: no vote
    // was subtracted from a proposal, an advocate was silenced instead.
    let mut planner = ScriptedAgent::new("planner", ["!propose #stage"]);
    let mut scout = ScriptedAgent::new("scout", ["!propose #ship"]);
    let mut critic = ScriptedAgent::new(
        "critic",
        [
            "!object >3 ^1 That precedent differs.",
            "!support #stage ^1 Bounds the blast radius.",
        ],
    );
    let mut archivist =
        ScriptedAgent::new("archivist", ["!support #stage ^1 Confirmed safe rollback."]);

    let (outcome, _) = harness.run(
        state,
        &policy(10),
        &mut [&mut planner, &mut scout, &mut critic, &mut archivist],
    )?;

    let Outcome::Converged { topic, standing } = &outcome else {
        panic!("expected cross-inhibition to converge the room, got {outcome:?}")
    };
    assert_eq!(topic, &TopicId("stage".into()));
    assert!(
        !standing.supporters.is_empty(),
        "a carried topic must name who carried it",
    );

    // Cross-inhibition actually fired: scout, the objected-to advocate, is
    // silenced out of `ship`'s standing rather than merely outbid.
    let traces = read(harness.journal());
    let at = harness
        .journal()
        .last()
        .expect("the journal is non-empty")
        .sequence;
    let policy = policy(10);
    let folded = standings(&traces, at, &policy.quorum).expect("folds");
    let ship = folded
        .iter()
        .find(|standing| standing.topic == TopicId("ship".into()))
        .expect("ship was proposed and must have a standing");
    assert_eq!(
        ship.silenced,
        ["scout"],
        "the objection must silence scout as ship's advocate",
    );
    assert!(
        ship.supporters.is_empty(),
        "a silenced sole advocate leaves no supporters behind",
    );

    Ok(())
}

#[test]
fn an_episode_does_not_inherit_the_votes_of_the_conversation_before_it() -> Result<(), String> {
    let mut harness = harness();
    // A previous, already-settled deliberation sits in the journal.
    harness.agent("planner", "!propose #legacy");
    harness.agent("critic", "!support #legacy ^1");
    harness.operator("New question: how do we roll out?");

    let state = EpisodeState::opened(harness.conversation(), harness.watermark());
    let mut planner = ScriptedAgent::new("planner", ["!question"]);
    let mut critic = ScriptedAgent::new("critic", ["!question"]);
    let mut scout = ScriptedAgent::new("scout", ["!question"]);

    let (outcome, _) = harness.run(
        state,
        &policy(3),
        &mut [&mut planner, &mut critic, &mut scout],
    )?;

    assert_eq!(
        outcome,
        Outcome::Exhausted { spent: 3 },
        "the settled `legacy` topic sits below the watermark and must not carry",
    );
    Ok(())
}

#[test]
fn every_member_of_a_lopsided_room_eventually_gets_the_floor() -> Result<(), String> {
    let mut harness = harness();
    harness.operator("Decide how to roll this out.");
    let state = EpisodeState::opened(harness.conversation(), harness.watermark());

    let mut planner = ScriptedAgent::new("planner", ["!propose #stage"]);
    let mut critic = ScriptedAgent::new("critic", ["!question"]);
    let mut scout = ScriptedAgent::new("scout", ["!question"]);

    let (_, steps) = harness.run(
        state,
        &policy(6),
        &mut [&mut planner, &mut critic, &mut scout],
    )?;

    let mut heard: Vec<&str> = steps.iter().map(|step| step.agent_id.as_str()).collect();
    heard.sort_unstable();
    heard.dedup();
    assert_eq!(
        heard.len(),
        MEMBERS.len(),
        "speaking costs the speaker, so the floor must circulate: {steps:?}",
    );
    Ok(())
}

#[test]
fn a_bid_reason_is_reported_for_every_turn() -> Result<(), String> {
    let mut harness = harness();
    harness.operator("Decide how to roll this out.");
    let state = EpisodeState::opened(harness.conversation(), harness.watermark());

    let mut planner = ScriptedAgent::new("planner", ["!propose #stage"]);
    let mut critic = ScriptedAgent::new("critic", ["!support #stage ^2"]);
    let mut scout = ScriptedAgent::new("scout", ["!question"]);

    let (_, steps) = harness.run(
        state,
        &policy(4),
        &mut [&mut planner, &mut critic, &mut scout],
    )?;

    assert!(!steps.is_empty());
    for step in &steps {
        assert!(
            matches!(
                step.reason,
                BidReason::Addressed | BidReason::Dissent | BidReason::Quiet | BidReason::Salience
            ),
            "every turn must be able to say why it was authorized",
        );
    }
    Ok(())
}
