//! Quorum, the one-way phase change from `Deliberate` to `Commit`, and
//! convergence: a room converges only once a `!commit` trace names the
//! carried topic strictly after the boundary the phase flip fixed, never on
//! phase alone — and only messages above the watermark, from current members,
//! can carry a topic there at all.

use super::super::*;
use super::support::{
    Room, converging, conversation, operator, run, said, sequential, speaking, spoke, state,
};
use crate::trace::TopicId;
use tinyhivemind::Sequence;

#[test]
fn quorum_flips_the_phase_once_and_then_converges() {
    let room = Room::new();
    let policy = sequential();

    // Deliberating with quorum reached: one commit turn is authorized.
    let (turn, next) = spoke(run(&room, &state(), &converging(), &policy));
    assert_eq!(turn.phase, Phase::Commit);
    assert_eq!(next.phase, Phase::Commit);

    // The commit-phase turn speaks, but records nothing: phase alone must
    // not be read as proof that the room recorded its decision.
    let mut transcript = converging();
    assert!(
        matches!(
            run(&room, &next, &transcript, &policy),
            HiveStep::Speak { .. },
        ),
        "a commit-phase turn that recorded no `!commit` must not converge",
    );

    // Once the authorized speaker actually commits the carried topic, the
    // episode reports its decision.
    transcript.push(said(4, &turn.agent_id, "!commit #stage Locking this in."));
    let HiveStep::Converged { topic, standing } = run(&room, &next, &transcript, &policy) else {
        panic!("expected convergence")
    };
    assert_eq!(topic, TopicId("stage".into()));
    assert_eq!(standing.supporters, ["planner", "critic"]);
}

#[test]
fn traces_from_non_members_do_not_manufacture_quorum() {
    // Neither author here is a member of this desk (or of the roster at
    // all), so their `!propose`/`!support` traces must not be folded into
    // standings. If they were, two non-members could manufacture quorum
    // nobody eligible actually holds.
    let room = Room::new();
    let transcript = vec![
        said(1, "ghost", "!propose #stage Rogue proposal."),
        said(2, "intruder", "!support #stage ^1 Rogue support."),
    ];
    let turn = speaking(run(&room, &state(), &transcript, &sequential()));
    assert_eq!(
        turn.phase,
        Phase::Deliberate,
        "non-member traces must not carry a topic to quorum",
    );
}

#[test]
fn a_commit_trace_before_the_commit_boundary_does_not_converge() {
    let room = Room::new();
    let policy = sequential();

    // A `!commit` for `#stage` already sits in the transcript before quorum
    // ever forms -- planted speculatively, or left over from an earlier
    // exchange. It must not be read as evidence that *this* commit-phase
    // turn recorded anything.
    let mut transcript = vec![
        said(1, "planner", "!commit #stage Locking this in early."),
        operator(2, "Decide how to roll this out."),
        said(3, "planner", "!propose #stage Stage the rollout."),
        said(4, "critic", "!support #stage ^3 Bounds the blast radius."),
    ];

    let (turn, next) = spoke(run(&room, &state(), &transcript, &policy));
    assert_eq!(turn.phase, Phase::Commit);

    // The authorized commit-phase turn itself does not commit.
    transcript.push(said(5, &turn.agent_id, "!question Anything else?"));
    assert!(
        matches!(
            run(&room, &next, &transcript, &policy),
            HiveStep::Speak { .. },
        ),
        "a `!commit` trace that predates the commit boundary must not converge the room",
    );
}

#[test]
fn the_commit_phase_is_one_way_when_support_later_decays_out() {
    let room = Room::new();
    let policy = EpisodePolicy {
        quorum: crate::quorum::QuorumPolicy {
            window: 2,
            ..crate::quorum::QuorumPolicy::DEFAULT
        },
        ..sequential()
    };
    let committing = EpisodeState {
        phase: Phase::Commit,
        ..state()
    };
    // The supporting traces have aged out of the window entirely.
    let mut transcript = converging();
    transcript.push(said(40, "scout", "!question"));

    let step = run(&room, &committing, &transcript, &policy);
    let turn = speaking(step);
    assert_eq!(
        turn.phase,
        Phase::Commit,
        "a room that has settled does not reopen because support decayed",
    );
}

#[test]
fn traces_at_or_below_the_watermark_are_context_not_votes() {
    let room = Room::new();
    let opened_late = EpisodeState::opened(conversation(), Sequence(3));
    // The whole converging exchange sits at or below the watermark.
    let step = run(&room, &opened_late, &converging(), &sequential());
    let turn = speaking(step);
    assert_eq!(
        turn.phase,
        Phase::Deliberate,
        "an episode must not inherit the quorum of the conversation before it",
    );
}
