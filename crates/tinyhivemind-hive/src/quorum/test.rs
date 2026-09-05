//! Unit tests for quorum counting and cross-inhibition.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

use crate::trace::read;
use tinyhivemind::{SessionAuthor, SessionMessage};

fn said(sequence: u64, author: &str, content: &str) -> SessionMessage {
    SessionMessage {
        sequence: Sequence(sequence),
        author: SessionAuthor::Agent {
            id: author.into(),
            label: author.into(),
        },
        content: content.into(),
    }
}

/// The shared policy for these tests, with refutation switched on.
///
/// The crate default leaves `refutation_cap` at `None` because the benchmark
/// scored the mechanism and it lost. Every test below that exercises the
/// mechanism has to turn it on, and the ones that check it is off say so.
fn policy(threshold: u32) -> QuorumPolicy {
    QuorumPolicy {
        threshold,
        window: 100,
        require_grounded: true,
        refutation_cap: Some(2),
        ..QuorumPolicy::DEFAULT
    }
}

fn fold(transcript: &[SessionMessage], policy: &QuorumPolicy) -> Vec<TopicStanding> {
    let at = transcript.last().map_or(Sequence(0), |m| m.sequence);
    standings(&read(transcript), at, policy).expect("folds")
}

fn standing<'a>(standings: &'a [TopicStanding], topic: &str) -> &'a TopicStanding {
    standings
        .iter()
        .find(|standing| standing.topic.as_str() == topic)
        .unwrap_or_else(|| panic!("no standing for {topic}"))
}

/// Two proposals, two grounded supporters each: a genuine tie.
fn deadlocked_transcript() -> Vec<SessionMessage> {
    vec![
        said(1, "planner", "!propose #stage Stage the rollout."),
        said(2, "scout", "!propose #ship Ship it at once."),
        said(3, "critic", "!support #stage ^1 Bounds the blast radius."),
        said(
            4,
            "archivist",
            "!support #ship ^2 We have shipped this before.",
        ),
    ]
}

#[test]
fn a_policy_and_standing_pin_their_wire_forms() {
    let value = serde_json::to_value(QuorumPolicy::DEFAULT).expect("serializes");
    assert_eq!(
        value,
        serde_json::json!({
            "threshold": 2,
            "window": 30,
            "require_grounded": true,
            "refutation_cap": null,
            "require_evidential": false,
        }),
    );
    assert_eq!(
        serde_json::from_value::<QuorumPolicy>(value).expect("deserializes"),
        QuorumPolicy::DEFAULT,
    );

    let standing = TopicStanding {
        topic: "stage".into(),
        supporters: vec!["planner".into()],
        silenced: vec!["scout".into()],
        refuted_by: vec!["auditor".into()],
        support: 1_400,
    };
    let value = serde_json::to_value(&standing).expect("serializes");
    assert_eq!(
        value,
        serde_json::json!({
            "topic": "stage",
            "supporters": ["planner"],
            "silenced": ["scout"],
            "refuted_by": ["auditor"],
            "support": 1_400,
        }),
    );
    assert_eq!(
        serde_json::from_value::<TopicStanding>(value).expect("deserializes"),
        standing,
    );
}

#[test]
fn consensus_pins_its_tagged_wire_form() {
    assert_eq!(
        serde_json::to_value(ConsensusState::Deliberating).expect("serializes"),
        serde_json::json!({ "state": "deliberating" }),
    );
    assert_eq!(
        serde_json::to_value(ConsensusState::Quorum {
            topic: "stage".into()
        })
        .expect("serializes"),
        serde_json::json!({ "state": "quorum", "topic": "stage" }),
    );
    assert_eq!(
        serde_json::to_value(ConsensusState::Deadlocked {
            topics: vec!["stage".into(), "ship".into()],
        })
        .expect("serializes"),
        serde_json::json!({ "state": "deadlocked", "topics": ["stage", "ship"] }),
    );
}

#[test]
fn the_default_policy_is_the_conservative_one() {
    assert_eq!(QuorumPolicy::default(), QuorumPolicy::DEFAULT);
    assert_eq!(QuorumPolicy::default().threshold, 2);
    // Both narrowing knobs are off by default, because the benchmark scored
    // them and they lost. See `docs/experiments/`.
    assert_eq!(QuorumPolicy::default().refutation_cap, None);
    assert!(!QuorumPolicy::default().require_evidential);
}

#[test]
fn refutations_are_recorded_but_cap_nothing_under_the_default_policy() {
    let transcript = [
        said(1, "planner", "!propose #stage Stage the rollout."),
        said(
            2,
            "critic",
            "!support #stage ^1 It bounds the blast radius.",
        ),
        said(3, "auditor", "!evidence The environment was retired."),
        said(4, "auditor", "!refute #stage ^3 Nowhere to stage it."),
        said(5, "scout", "!refute #stage ^3 Confirmed."),
    ];
    let default = QuorumPolicy {
        window: 100,
        ..QuorumPolicy::DEFAULT
    };
    let standings = fold(&transcript, &default);
    let held = standing(&standings, "stage");
    // The room's disagreement is on the record either way. Only the *effect*
    // is opt-in.
    assert_eq!(held.refuted_by, ["auditor", "scout"]);
    assert!(held.carried(&default));
}

#[test]
fn a_zero_threshold_or_window_is_rejected() {
    let zero_threshold = QuorumPolicy {
        threshold: 0,
        ..QuorumPolicy::DEFAULT
    };
    let error = standings(&[], Sequence(1), &zero_threshold).expect_err("zero threshold");
    assert_eq!(error.to_string(), "quorum threshold must not be zero");

    let zero_window = QuorumPolicy {
        window: 0,
        ..QuorumPolicy::DEFAULT
    };
    let error = standings(&[], Sequence(1), &zero_window).expect_err("zero window");
    assert_eq!(error.to_string(), "quorum window must not be zero");

    let zero_cap = QuorumPolicy {
        refutation_cap: Some(0),
        ..QuorumPolicy::DEFAULT
    };
    let error = standings(&[], Sequence(1), &zero_cap).expect_err("zero refutation cap");
    assert_eq!(error.to_string(), "refutation cap must not be zero");
}

#[test]
fn a_proposer_supports_its_own_topic() {
    let standings = fold(&[said(1, "planner", "!propose #stage")], &policy(2));
    assert_eq!(standing(&standings, "stage").supporters, ["planner"]);
}

#[test]
fn distinct_supporters_carry_a_topic_and_repeat_support_does_not() {
    let repeated = fold(
        &[
            said(1, "planner", "!propose #stage"),
            said(2, "planner", "!support #stage ^1"),
            said(3, "planner", "!support #stage ^1"),
        ],
        &policy(2),
    );
    assert_eq!(standing(&repeated, "stage").supporters, ["planner"]);
    assert_eq!(
        consensus(&repeated, &policy(2)),
        ConsensusState::Deliberating
    );

    let distinct = fold(
        &[
            said(1, "planner", "!propose #stage"),
            said(2, "critic", "!support #stage ^1"),
        ],
        &policy(2),
    );
    assert_eq!(
        consensus(&distinct, &policy(2)),
        ConsensusState::Quorum {
            topic: "stage".into()
        },
    );
}

#[test]
fn an_ungrounded_support_moves_neither_the_supporters_nor_the_weight() {
    let transcript = [
        said(1, "planner", "!propose #stage"),
        said(2, "critic", "!support #stage"),
    ];
    let grounded_required = fold(&transcript, &policy(2));
    let held = standing(&grounded_required, "stage");
    assert_eq!(held.supporters, ["planner"]);
    assert_eq!(held.support, importance(TraceKind::Propose));

    // The same input under a policy that does not require grounds carries.
    let lax = QuorumPolicy {
        require_grounded: false,
        ..policy(2)
    };
    let permissive = fold(&transcript, &lax);
    assert_eq!(
        standing(&permissive, "stage").supporters,
        ["planner", "critic"]
    );
}

#[test]
fn support_outside_the_window_stops_counting() {
    let transcript = [
        said(1, "planner", "!propose #stage"),
        said(50, "critic", "!support #stage ^1"),
    ];
    let narrow = QuorumPolicy {
        window: 10,
        ..policy(2)
    };
    let standings = fold(&transcript, &narrow);
    assert_eq!(standing(&standings, "stage").supporters, ["critic"]);
    assert_eq!(consensus(&standings, &narrow), ConsensusState::Deliberating);
}

#[test]
fn a_trace_without_a_topic_or_an_agent_author_is_not_counted() {
    let transcript = [
        said(1, "planner", "!propose #stage"),
        said(2, "critic", "!support ^1"),
        SessionMessage {
            sequence: Sequence(3),
            author: SessionAuthor::Operator,
            content: "!support #stage ^1".into(),
        },
    ];
    let standings = fold(&transcript, &policy(2));
    assert_eq!(standing(&standings, "stage").supporters, ["planner"]);
}

// --- Cross-inhibition: the mechanism, and the proof that it is load-bearing ---

#[test]
fn two_equally_supported_topics_deadlock() {
    let standings = fold(&deadlocked_transcript(), &policy(2));
    assert_eq!(
        consensus(&standings, &policy(2)),
        ConsensusState::Deadlocked {
            topics: vec!["stage".into(), "ship".into()],
        },
    );
}

#[test]
fn a_grounded_objection_silences_an_advocate_and_breaks_the_deadlock() {
    let mut transcript = deadlocked_transcript();
    // The objection names archivist's supporting message, not the topic.
    transcript.push(said(
        5,
        "critic",
        "!object >4 ^3 That precedent was a different system.",
    ));

    let standings = fold(&transcript, &policy(2));
    let ship = standing(&standings, "ship");
    assert_eq!(ship.silenced, ["archivist"]);
    assert_eq!(ship.supporters, ["scout"]);
    assert_eq!(
        consensus(&standings, &policy(2)),
        ConsensusState::Quorum {
            topic: "stage".into()
        },
        "silencing one advocate must break the tie the same input deadlocks on",
    );
}

#[test]
fn silencing_the_last_advocate_zeroes_a_topics_weight() {
    let transcript = [
        said(1, "planner", "!propose #stage"),
        said(2, "critic", "!object >1 ^0 Not this."),
    ];
    let standings = fold(&transcript, &policy(2));
    let held = standing(&standings, "stage");
    assert!(held.supporters.is_empty());
    assert_eq!(held.silenced, ["planner"]);
    assert_eq!(held.support, 0);
}

#[test]
fn an_objection_cannot_silence_its_own_author() {
    // Otherwise an agent could retract a peer's support by objecting to itself.
    let transcript = [
        said(1, "planner", "!propose #stage"),
        said(2, "planner", "!object >1 ^1 Second thoughts."),
    ];
    assert_eq!(
        standing(&fold(&transcript, &policy(2)), "stage").supporters,
        ["planner"]
    );
}

#[test]
fn an_ungrounded_objection_silences_nobody() {
    let transcript = [
        said(1, "planner", "!propose #stage"),
        said(2, "critic", "!object >1"),
    ];
    let standings = fold(&transcript, &policy(2));
    assert!(standing(&standings, "stage").silenced.is_empty());
}

#[test]
fn an_objection_at_an_unknown_or_absent_target_silences_nobody() {
    let transcript = [
        said(1, "planner", "!propose #stage"),
        said(2, "critic", "!object >99 ^1"),
        said(3, "critic", "!object ^1"),
    ];
    assert!(
        standing(&fold(&transcript, &policy(2)), "stage")
            .silenced
            .is_empty()
    );
}

#[test]
fn an_objection_silences_every_topic_a_targeted_message_advocated() {
    // planner's single message advocates two topics at two offsets. An
    // objection naming that message must silence planner as the advocate of
    // *both* topics, not just whichever one a sequence-keyed map happened to
    // remember last.
    let transcript = vec![
        said(1, "planner", "!propose #stage\n!propose #ship"),
        said(2, "critic", "!support #stage ^1 Bounds the blast radius."),
        said(3, "archivist", "!support #ship ^1 We shipped this before."),
        said(4, "auditor", "!object >1 ^1 Neither precedent holds."),
    ];
    let standings = fold(&transcript, &policy(2));

    let stage = standing(&standings, "stage");
    assert!(
        !stage.supporters.contains(&"planner".to_owned()),
        "planner advocated #stage in the targeted message and must be silenced there too",
    );
    assert!(stage.silenced.contains(&"planner".to_owned()));

    let ship = standing(&standings, "ship");
    assert!(!ship.supporters.contains(&"planner".to_owned()));
    assert!(ship.silenced.contains(&"planner".to_owned()));
}

#[test]
fn silencing_one_of_two_advocates_leaves_only_the_survivors_weight() {
    // planner proposes (900) and critic supports (500). Objecting to planner
    // alone must drop planner's 900 from the topic's weight entirely, not
    // just leave the topic non-empty with both contributions still summed.
    let transcript = vec![
        said(1, "planner", "!propose #stage Stage the rollout."),
        said(2, "critic", "!support #stage ^1 Bounds the blast radius."),
        said(3, "auditor", "!object >1 ^1 Reconsider the precedent."),
    ];
    let standings = fold(&transcript, &policy(2));
    let stage = standing(&standings, "stage");

    assert_eq!(
        stage.supporters,
        ["critic"],
        "planner is silenced, critic remains",
    );
    assert_eq!(
        stage.support, 500,
        "the surviving weight must be critic's contribution alone, not \
         planner's silenced 900 plus critic's 500",
    );
}

#[test]
fn a_repeated_objection_records_an_advocate_once() {
    let transcript = [
        said(1, "planner", "!propose #stage"),
        said(2, "critic", "!object >1 ^0"),
        said(3, "scout", "!object >1 ^0"),
    ];
    assert_eq!(
        standing(&fold(&transcript, &policy(2)), "stage").silenced,
        ["planner"]
    );
}

// --- Fold discipline ---

#[test]
fn standings_are_order_independent() {
    let transcript = {
        let mut transcript = deadlocked_transcript();
        transcript.push(said(5, "critic", "!object >4 ^3"));
        transcript
    };
    let at = Sequence(5);
    let ordered = read(&transcript);

    let mut shuffled = ordered.clone();
    shuffled.reverse();
    let mut rotated = ordered.clone();
    rotated.rotate_left(2);

    let expected = standings(&ordered, at, &policy(2)).expect("folds");
    for permutation in [shuffled, rotated] {
        assert_eq!(
            standings(&permutation, at, &policy(2)).expect("folds"),
            expected,
            "a reordered fold must land in the same place, topic order included",
        );
    }
}

#[test]
fn standings_are_idempotent_over_duplicated_traces() {
    let transcript = deadlocked_transcript();
    let at = Sequence(4);
    let once = read(&transcript);
    let twice: Vec<_> = once.iter().cloned().chain(once.iter().cloned()).collect();
    assert_eq!(
        standings(&twice, at, &policy(2)).expect("folds"),
        standings(&once, at, &policy(2)).expect("folds"),
    );
}

#[test]
fn an_empty_medium_is_deliberating() {
    let standings = standings(&[], Sequence(0), &policy(2)).expect("folds");
    assert!(standings.is_empty());
    assert_eq!(
        consensus(&standings, &policy(2)),
        ConsensusState::Deliberating
    );
}

#[test]
fn carried_reports_whether_a_standing_reached_the_threshold() {
    let standing = TopicStanding {
        topic: "stage".into(),
        supporters: vec!["a".into(), "b".into()],
        silenced: Vec::new(),
        refuted_by: Vec::new(),
        support: 1,
    };
    assert!(standing.carried(&policy(2)));
    assert!(!standing.carried(&policy(3)));
}

/// Two grounded supporters, and a fact one member can point at.
fn contested_transcript() -> Vec<SessionMessage> {
    vec![
        said(1, "planner", "!propose #stage Stage the rollout."),
        said(
            2,
            "critic",
            "!support #stage ^1 It bounds the blast radius.",
        ),
        said(
            3,
            "auditor",
            "!evidence The staging environment was retired in March.",
        ),
    ]
}

#[test]
fn a_refutation_needs_both_a_topic_and_a_citation() {
    // The marker parses only with both qualifiers. Without either it deposits
    // nothing at all, rather than a trace that could cap a topic on nothing.
    let traces = read(&[
        said(1, "auditor", "!refute #stage ^0 Grounded and named."),
        said(2, "auditor", "!refute #stage Names a topic, cites nothing."),
        said(3, "auditor", "!refute ^1 Cites something, names no topic."),
        said(4, "auditor", "!refute Neither."),
    ]);
    assert_eq!(traces.len(), 1);
    assert_eq!(traces[0].kind, TraceKind::Refute);
    assert_eq!(traces[0].sequence, Sequence(1));
    assert!(traces[0].grounded());
}

#[test]
fn refuters_below_the_cap_leave_a_carried_topic_carried() {
    let mut transcript = contested_transcript();
    transcript.push(said(4, "auditor", "!refute #stage ^3 Nowhere to stage it."));
    let standings = fold(&transcript, &policy(2));
    let held = standing(&standings, "stage");

    assert_eq!(held.refuted_by, ["auditor"]);
    // One refuter is below the default cap of two, so nothing is capped, and
    // no supporter was removed: refutation never silences anybody.
    assert_eq!(held.supporters, ["planner", "critic"]);
    assert!(held.carried(&policy(2)));
}

#[test]
fn the_refutation_cap_takes_a_topic_out_of_contention_without_silencing_anyone() {
    let mut transcript = contested_transcript();
    transcript.push(said(4, "auditor", "!refute #stage ^3 Nowhere to stage it."));
    transcript.push(said(5, "scout", "!refute #stage ^3 Confirmed, it is gone."));
    let standings = fold(&transcript, &policy(2));
    let held = standing(&standings, "stage");

    assert_eq!(held.refuted_by, ["auditor", "scout"]);
    assert!(held.silenced.is_empty());
    // Everything the room did survives in the standing. Only `carried` moves.
    assert_eq!(held.supporters, ["planner", "critic"]);
    assert_eq!(
        held.support,
        importance(TraceKind::Propose) + importance(TraceKind::Support),
    );
    assert!(!held.carried(&policy(2)));
    assert_eq!(
        consensus(&standings, &policy(2)),
        ConsensusState::Deliberating,
    );
}

#[test]
fn one_refutation_ends_a_deadlock_that_would_have_cost_a_turn_per_advocate() {
    // This is the shape the live rooms could not write: a fact that kills one
    // of two tied hypotheses, in one turn rather than one turn per advocate.
    let mut transcript = deadlocked_transcript();
    assert!(matches!(
        consensus(&fold(&transcript, &policy(2)), &policy(2)),
        ConsensusState::Deadlocked { .. },
    ));

    transcript.push(said(5, "auditor", "!evidence The environment was retired."));
    transcript.push(said(6, "auditor", "!refute #stage ^5 Nowhere to stage it."));
    let one_refuter = QuorumPolicy {
        refutation_cap: Some(1),
        ..policy(2)
    };
    assert_eq!(
        consensus(&fold(&transcript, &one_refuter), &one_refuter),
        ConsensusState::Quorum {
            topic: "ship".into()
        },
    );
}

#[test]
fn repeated_refutation_by_one_member_counts_once() {
    let mut transcript = contested_transcript();
    transcript.push(said(4, "auditor", "!refute #stage ^3 Nowhere to stage it."));
    transcript.push(said(5, "auditor", "!refute #stage ^3 Still nowhere."));
    let held = fold(&transcript, &policy(2));
    let held = standing(&held, "stage");
    assert_eq!(held.refuted_by, ["auditor"]);
    assert!(held.carried(&policy(2)));
}

#[test]
fn refuting_a_topic_nobody_advocated_is_inert() {
    // A refutation attaches to a topic some member put on the floor. Otherwise
    // one member could manufacture a standing nobody else ever mentioned.
    let standings = fold(
        &[
            said(1, "auditor", "!evidence Nobody proposed this."),
            said(2, "auditor", "!refute #phantom ^1 Refuting thin air."),
        ],
        &policy(2),
    );
    assert!(standings.is_empty());
}

#[test]
fn a_member_that_both_supports_and_refutes_a_topic_is_only_a_refuter() {
    let standings = fold(
        &[
            said(1, "planner", "!propose #stage Stage the rollout."),
            said(2, "critic", "!support #stage ^1 Agreed."),
            said(3, "critic", "!evidence The environment was retired."),
            said(4, "critic", "!refute #stage ^3 I was wrong about this."),
        ],
        &policy(2),
    );
    let held = standing(&standings, "stage");
    assert_eq!(held.supporters, ["planner"]);
    assert_eq!(held.refuted_by, ["critic"]);
    assert_eq!(held.support, importance(TraceKind::Propose));
}

#[test]
fn refutations_fold_commutatively_and_idempotently() {
    let mut transcript = contested_transcript();
    transcript.push(said(4, "auditor", "!refute #stage ^3 Nowhere to stage it."));
    transcript.push(said(5, "scout", "!refute #stage ^3 Confirmed."));
    let at = Sequence(5);

    let forward = read(&transcript);
    let mut reversed = forward.clone();
    reversed.reverse();
    let doubled: Vec<_> = forward
        .iter()
        .cloned()
        .chain(forward.iter().cloned())
        .collect();

    let expected = standings(&forward, at, &policy(2)).expect("folds");
    assert_eq!(
        standings(&reversed, at, &policy(2)).expect("folds"),
        expected
    );
    assert_eq!(
        standings(&doubled, at, &policy(2)).expect("folds"),
        expected
    );
}

#[test]
fn a_refutation_outside_the_window_stops_capping() {
    let mut transcript = contested_transcript();
    transcript.push(said(4, "auditor", "!refute #stage ^3 Nowhere to stage it."));
    transcript.push(said(5, "scout", "!refute #stage ^3 Confirmed."));
    transcript.push(said(40, "planner", "!propose #stage Raising it again."));
    transcript.push(said(41, "critic", "!support #stage ^40 Still worth it."));

    let narrow = QuorumPolicy {
        window: 5,
        ..policy(2)
    };
    let standings = standings(&read(&transcript), Sequence(41), &narrow).expect("folds");
    let held = standing(&standings, "stage");
    assert!(held.refuted_by.is_empty());
    assert!(held.carried(&narrow));
}

fn evidential() -> QuorumPolicy {
    QuorumPolicy {
        require_evidential: true,
        ..policy(2)
    }
}

#[test]
fn support_grounded_only_in_another_opinion_does_not_count_as_evidential() {
    // A cascade with a citation on it: every link is grounded, and the chain
    // bottoms out in an opinion rather than in a fact.
    let transcript = [
        said(1, "planner", "!propose #stage Stage the rollout."),
        said(
            2,
            "critic",
            "!support #stage ^1 The planner is usually right.",
        ),
        said(3, "scout", "!support #stage ^2 The critic agrees."),
    ];
    assert_eq!(
        standing(&fold(&transcript, &evidential()), "stage").supporters,
        ["planner"],
    );
    // The same chain counts when only a citation is required.
    assert_eq!(
        standing(&fold(&transcript, &policy(2)), "stage").supporters,
        ["planner", "critic", "scout"],
    );
}

#[test]
fn support_whose_chain_reaches_a_fact_counts_as_evidential() {
    let transcript = [
        said(
            1,
            "auditor",
            "!evidence In-flight requests sit at 24 to 31.",
        ),
        said(2, "planner", "!propose #pool The pool caps at twenty."),
        said(3, "critic", "!support #pool ^1 The numbers line up."),
        // A chain two links long still reaches the fact at sequence 1.
        said(
            4,
            "scout",
            "!support #pool ^3 Following the critic's grounds.",
        ),
    ];
    let standings = fold(&transcript, &evidential());
    let held = standing(&standings, "pool");
    assert_eq!(held.supporters, ["planner", "critic", "scout"]);
    assert!(held.carried(&evidential()));
}

#[test]
fn a_citation_cycle_terminates_and_reads_as_social() {
    // Two supports citing each other, and nothing else. The visited set is what
    // stops the resolution recurring; the chain reaches no fact, so neither
    // support counts.
    let transcript = [
        said(1, "planner", "!propose #stage Stage it."),
        said(2, "critic", "!support #stage ^3 Circular."),
        said(3, "scout", "!support #stage ^2 Also circular."),
    ];
    assert_eq!(
        standing(&fold(&transcript, &evidential()), "stage").supporters,
        ["planner"],
    );
}

#[test]
fn a_chain_that_leaves_the_window_reads_as_social() {
    // The citation is real, and it is outside the window. Chasing it would make
    // a member's standing depend on how far back it happened to have paged.
    let transcript = [
        said(
            1,
            "auditor",
            "!evidence In-flight requests sit at 24 to 31.",
        ),
        said(40, "planner", "!propose #pool The pool caps at twenty."),
        said(41, "critic", "!support #pool ^1 The numbers line up."),
    ];
    let narrow = QuorumPolicy {
        window: 5,
        ..evidential()
    };
    let standings = standings(&read(&transcript), Sequence(41), &narrow).expect("folds");
    assert_eq!(standing(&standings, "pool").supporters, ["planner"]);
}

#[test]
fn an_objection_from_a_member_with_no_evidence_silences_nobody() {
    let transcript = [
        said(1, "planner", "!propose #stage Stage the rollout."),
        said(2, "auditor", "!evidence The environment was retired."),
        said(3, "critic", "!support #stage ^2 Worth it anyway."),
        // The scout has put no fact on the floor, so its objection is inert
        // under `require_evidential`, and lands under the weaker policy.
        said(4, "scout", "!object >3 ^1 I disagree with the critic."),
    ];
    assert_eq!(
        standing(&fold(&transcript, &evidential()), "stage").supporters,
        ["planner", "critic"],
    );
    assert_eq!(
        standing(&fold(&transcript, &policy(2)), "stage").silenced,
        ["critic"],
    );
}

#[test]
fn requiring_evidential_grounds_implies_requiring_grounds_at_all() {
    let transcript = [
        said(1, "planner", "!propose #stage Stage the rollout."),
        said(2, "critic", "!support #stage No citation at all."),
    ];
    let lax_but_evidential = QuorumPolicy {
        require_grounded: false,
        require_evidential: true,
        ..policy(2)
    };
    assert_eq!(
        standing(&fold(&transcript, &lax_but_evidential), "stage").supporters,
        ["planner"],
    );
}

#[test]
fn a_deferral_moves_no_support_and_creates_no_standing() {
    let transcript = [
        said(1, "planner", "!propose #stage Stage the rollout."),
        said(2, "critic", "!support #stage ^1 Bounds the blast radius."),
        // Abstention is not a vote in either direction, and it must not be
        // able to open a standing for a topic nobody advocated.
        said(3, "scout", "!defer #stage ^1 Not my area."),
        said(4, "scout", "!defer #pool The archivist measured this."),
    ];
    let folded = fold(&transcript, &policy(2));
    assert_eq!(folded.len(), 1);
    let stage = standing(&folded, "stage");
    assert_eq!(stage.supporters, ["planner", "critic"]);
    assert!(stage.silenced.is_empty());
    assert!(stage.refuted_by.is_empty());

    // The same room without the two deferrals folds identically.
    let without = fold(&transcript[..2], &policy(2));
    assert_eq!(folded, without);
}
