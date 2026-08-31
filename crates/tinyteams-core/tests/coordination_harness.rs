//! End-to-end and edge-case tests for a host coordinating multiple agents.

#[path = "support/harness.rs"]
mod harness;
#[path = "support/scripted_agent.rs"]
mod scripted_agent;

use harness::ChatHarness;
use scripted_agent::ScriptedAgent;

#[test]
fn planner_critic_and_synthesizer_build_on_one_shared_transcript() -> Result<(), String> {
    let mut harness = ChatHarness::default();
    let mut planner = ScriptedAgent::new(
        "planner",
        [Ok("Plan: gather evidence, test the claim, then summarize.")],
    );
    let mut critic = ScriptedAgent::new(
        "critic",
        [Ok(
            "Risk: the plan needs a negative control and a stop condition.",
        )],
    );
    let mut synthesizer = ScriptedAgent::new(
        "synthesizer",
        [Ok(
            "Decision: add a negative control, stop on contradictory evidence, and summarize.",
        )],
    );

    harness.send(None, "operator", "Find a reliable answer together.");
    harness.dispatch(Some("main"), &mut planner)?;
    harness.dispatch(Some("General"), &mut critic)?;
    harness.dispatch(Some("GENERAL"), &mut synthesizer)?;

    let transcript = harness.transcript(None);
    assert_eq!(transcript.len(), 4);
    assert_eq!(
        transcript
            .iter()
            .map(|message| message.author.as_str())
            .collect::<Vec<_>>(),
        ["operator", "planner", "critic", "synthesizer"],
    );
    assert_eq!(planner.calls()[0].len(), 1);
    assert_eq!(critic.calls()[0].len(), 2);
    assert_eq!(synthesizer.calls()[0].len(), 3);
    assert_eq!(synthesizer.calls()[0][1].author, "planner");
    assert_eq!(synthesizer.calls()[0][2].author, "critic");
    Ok(())
}

#[test]
fn one_dispatch_runs_exactly_one_agent_turn() -> Result<(), String> {
    let mut harness = ChatHarness::default();
    let mut selected = ScriptedAgent::new("selected", [Ok("handled")]);
    let unselected = ScriptedAgent::new("unselected", [Ok("must not run")]);

    harness.send(Some("engineering"), "operator", "@selected investigate");
    harness.dispatch(Some("engineering"), &mut selected)?;

    assert_eq!(selected.calls().len(), 1);
    assert!(unselected.calls().is_empty());
    assert_eq!(harness.journal().len(), 2);
    Ok(())
}

#[test]
fn a_failed_agent_turn_does_not_append_a_phantom_message() {
    let mut harness = ChatHarness::default();
    let mut failing = ScriptedAgent::new("critic", [Err("model unavailable")]);
    harness.send(Some("engineering"), "operator", "Review the proposal.");

    let result = harness.dispatch(Some("engineering"), &mut failing);

    assert_eq!(result, Err("model unavailable".to_owned()));
    assert_eq!(failing.calls().len(), 1);
    assert_eq!(harness.journal().len(), 1);
}

#[test]
fn named_desks_do_not_leak_context_into_each_other() -> Result<(), String> {
    let mut harness = ChatHarness::default();
    let mut design_agent = ScriptedAgent::new("designer", [Ok("design reply")]);
    harness.send(
        Some("engineering"),
        "engineer",
        "private engineering context",
    );
    harness.send(Some("design"), "designer", "public design context");

    harness.dispatch(Some("design"), &mut design_agent)?;

    let received = &design_agent.calls()[0];
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].content, "public design context");
    assert_eq!(harness.transcript(Some("engineering")).len(), 1);
    assert_eq!(harness.transcript(Some("design")).len(), 2);
    Ok(())
}

#[test]
fn named_desk_ids_remain_case_sensitive_in_the_harness() {
    let mut harness = ChatHarness::default();
    harness.send(Some("Research"), "upper", "upper-case desk");
    harness.send(Some("research"), "lower", "lower-case desk");

    assert_eq!(harness.transcript(Some("Research")).len(), 1);
    assert_eq!(harness.transcript(Some("research")).len(), 1);
}

#[test]
fn sequence_numbers_are_monotonic_across_desk_surfaces() {
    let mut harness = ChatHarness::default();

    let first = harness.send(None, "operator", "first");
    let second = harness.send(Some("engineering"), "engineer", "second");
    let third = harness.send(Some("main"), "planner", "third");

    assert_eq!([first, second, third], [0, 1, 2]);
    assert_eq!(
        harness
            .journal()
            .iter()
            .map(|message| message.sequence)
            .collect::<Vec<_>>(),
        [0, 1, 2],
    );
}

#[test]
fn an_agent_without_a_script_reports_an_error_without_appending() {
    let mut harness = ChatHarness::default();
    let mut exhausted = ScriptedAgent::new("planner", []);

    let result = harness.dispatch(None, &mut exhausted);

    assert_eq!(result, Err("planner has no scripted response".to_owned()),);
    assert!(harness.journal().is_empty());
}
