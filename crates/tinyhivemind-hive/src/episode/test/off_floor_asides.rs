//! Private asides: an aside carries information, never support, so a trace
//! deposited inside one adds no supporter and moves no standing — the rule is
//! uniform across every reader — and a turn-holder outside an aside sees a
//! stub where a member of it sees the content.

use super::super::*;
use super::support::{MEMBERS, Room, aside, converging, operator, run, said, speaking, state};
use crate::quorum::QuorumPolicy;
use tinyhivemind::Sequence;

#[test]
fn a_trace_inside_an_aside_adds_no_supporter() {
    // The rule that keeps this a hive-mind primitive rather than a partition:
    // an aside carries information, never support. Two agents agreeing
    // privately have not moved the room.
    let room = Room::new();
    let policy = EpisodePolicy {
        quorum: QuorumPolicy {
            threshold: 2,
            ..QuorumPolicy::DEFAULT
        },
        blind_round: false,
        ..sequential()
    };

    let open = vec![
        operator(1, "Decide how to roll this out."),
        said(2, "planner", "!propose #stage Stage the rollout."),
        said(3, "critic", "!support #stage ^2 Bounds the blast radius."),
    ];
    let mut private = open.clone();
    private.push(aside(
        4,
        "scout",
        &["planner"],
        "!support #stage ^2 Quietly, I agree.",
    ));

    // The private support reaches quorum's threshold on paper and must not
    // move the standing, so the room takes the same step either way.
    assert_eq!(
        format!("{:?}", run(&room, &state(), &open, &policy)),
        format!("{:?}", run(&room, &state(), &private, &policy)),
    );
}

#[test]
fn surfacing_the_same_support_in_the_open_does_count() {
    // The other half of the rule: an aside is a staging area, and the way to
    // make it count is to spend a desk-visible turn saying so.
    let room = Room::new();
    let policy = EpisodePolicy {
        quorum: QuorumPolicy {
            threshold: 2,
            ..QuorumPolicy::DEFAULT
        },
        blind_round: false,
        ..sequential()
    };
    let transcript = vec![
        operator(1, "Decide how to roll this out."),
        said(2, "planner", "!propose #stage Stage the rollout."),
        aside(3, "scout", &["planner"], "!support #stage ^2 Quietly."),
        said(4, "scout", "!support #stage ^2 Bounds the blast radius."),
    ];
    assert!(matches!(
        run(&room, &state(), &transcript, &policy),
        HiveStep::Converged { .. } | HiveStep::Speak { .. },
    ));
    // The private line contributed nothing, so the standing is exactly what it
    // would have been had the aside never been written.
    let without: Vec<SessionMessage> = transcript
        .iter()
        .filter(|message| message.audience.is_desk())
        .cloned()
        .collect();
    let supporters = |slice: &[SessionMessage]| {
        crate::quorum::standings(&crate::trace::read(slice), Sequence(4), &policy.quorum)
            .expect("stands")
            .into_iter()
            .find(|standing| standing.topic.as_str() == "stage")
            .expect("the staged option")
            .supporters
    };
    assert_eq!(supporters(&transcript), supporters(&without));
    // And `scout` is there because of its open line, not its private one.
    assert!(supporters(&transcript).contains(&"scout".to_owned()));
}

#[test]
fn every_participant_counts_the_same_medium() {
    // Quorum answers for the room, not for a reader. The filter that drops an
    // aside from the fold is uniform, so there is exactly one standing.
    let transcript = vec![
        said(1, "planner", "!propose #stage Stage it."),
        aside(2, "critic", &["planner"], "!support #stage ^1 Quietly."),
        said(3, "scout", "!support #stage ^1 In the open."),
    ];
    let traces = crate::trace::read(&transcript);
    let standings =
        crate::quorum::standings(&traces, Sequence(3), &QuorumPolicy::DEFAULT).expect("stands");
    let stage = standings
        .iter()
        .find(|standing| standing.topic.as_str() == "stage")
        .expect("the staged option");
    // `planner` for proposing and `scout` for supporting in the open. `critic`
    // supported privately and is absent, for every reader alike.
    assert_eq!(
        stage.supporters,
        vec!["planner".to_owned(), "scout".to_owned()],
    );
}

#[test]
fn a_turn_holder_outside_an_aside_sees_a_stub_and_a_member_sees_the_content() {
    let room = Room::new();
    let policy = EpisodePolicy {
        blind_round: false,
        ..sequential()
    };
    let transcript = vec![
        operator(1, "Decide how to roll this out."),
        aside(2, "planner", &["critic"], "Between us: I am unsure."),
        aside(3, "critic", &["planner"], "So am I."),
        said(4, "planner", "The rollback path is the risk."),
    ];
    let turn = speaking(run(&room, &state(), &transcript, &policy));

    let outsider = HiveTurn {
        agent_id: "scout".into(),
        ..turn.clone()
    };
    let seen = project_for(&outsider, &transcript);
    assert_eq!(seen.len(), 3);
    assert_eq!(seen[1].readable(), None);
    assert_eq!(seen[1].elided.as_ref().expect("a stub").messages, 2);
    assert_eq!(
        seen[1].elided.as_ref().expect("a stub").settled_at,
        Some(Sequence(4)),
    );

    let member = HiveTurn {
        agent_id: "critic".into(),
        ..turn
    };
    let seen = project_for(&member, &transcript);
    assert_eq!(seen.len(), 4);
    assert!(seen.iter().all(|message| message.elided.is_none()));
}

#[test]
fn a_transcript_with_no_aside_projects_the_same_for_every_turn_holder() {
    let room = Room::new();
    let policy = EpisodePolicy {
        blind_round: false,
        ..sequential()
    };
    let transcript = converging();
    let turn = speaking(run(&room, &state(), &transcript, &policy));
    let baseline = project_for(&turn, &transcript);
    for id in MEMBERS {
        let other = HiveTurn {
            agent_id: id.into(),
            ..turn.clone()
        };
        assert_eq!(project_for(&other, &transcript), baseline);
    }
}
