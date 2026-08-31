//! Unit tests for the episode state machine.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

use crate::{attention::BidReason, trace::TopicId};
use tinyteams::{
    Conversation, Sequence,
    desk::{Desk, DeskSet, ResponderMode},
    roster::{Roster, RosterMember},
};

const MEMBERS: [&str; 3] = ["planner", "critic", "scout"];

fn member(id: &str) -> RosterMember {
    RosterMember {
        id: id.into(),
        name: Some(id.into()),
    }
}

fn roster_members() -> Vec<RosterMember> {
    MEMBERS.iter().map(|id| member(id)).collect()
}

fn desks() -> Vec<Desk> {
    vec![Desk {
        id: "engineering".into(),
        name: "Engineering".into(),
        description: None,
        members: MEMBERS.iter().map(|id| (*id).to_owned()).collect(),
        responder_mode: ResponderMode::Auto,
    }]
}

fn conversation() -> Conversation {
    Conversation {
        desk_id: "engineering".into(),
        desk_name: "Engineering".into(),
        thread_root: None,
    }
}

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

fn operator(sequence: u64, content: &str) -> SessionMessage {
    SessionMessage {
        sequence: Sequence(sequence),
        author: SessionAuthor::Operator,
        content: content.into(),
    }
}

struct Room {
    members: Vec<RosterMember>,
    desks: Vec<Desk>,
    retired: Vec<String>,
}

impl Room {
    fn new() -> Self {
        Self {
            members: roster_members(),
            desks: desks(),
            retired: Vec::new(),
        }
    }

    fn roster(&self) -> Roster<'_> {
        Roster::new(&self.members, &[], &self.retired)
    }

    fn desk_set(&self) -> DeskSet<'_> {
        DeskSet::new(&self.desks, &[], &[], &[], &self.retired)
    }
}

fn state() -> EpisodeState {
    EpisodeState::opened(conversation(), Sequence(0))
}

fn run(
    room: &Room,
    state: &EpisodeState,
    transcript: &[SessionMessage],
    policy: &EpisodePolicy,
) -> HiveStep {
    step(state, transcript, &room.roster(), &room.desk_set(), policy).expect("steps")
}

fn speaking(step: HiveStep) -> HiveTurn {
    let HiveStep::Speak { turn } = step else {
        panic!("expected a turn, got {step:?}")
    };
    *turn
}

/// A room with two grounded supporters behind one proposal.
fn converging() -> Vec<SessionMessage> {
    vec![
        operator(1, "Decide how to roll this out."),
        said(2, "planner", "!propose #stage Stage the rollout."),
        said(3, "critic", "!support #stage ^2 Bounds the blast radius."),
    ]
}

/// A room with two proposals each holding two grounded supporters.
fn deadlocked() -> Vec<SessionMessage> {
    vec![
        said(1, "planner", "!propose #stage"),
        said(2, "scout", "!propose #ship"),
        said(3, "critic", "!support #stage ^1"),
        said(4, "planner", "!support #ship ^2"),
    ]
}

// --- Wire forms ---

#[test]
fn the_policy_and_state_pin_their_wire_forms() {
    let value = serde_json::to_value(EpisodePolicy::DEFAULT).expect("serializes");
    assert_eq!(
        value,
        serde_json::json!({
            "turn_budget": 12,
            "blind_round": true,
            "dominance_cap": 50,
            "repetition_cap": 3,
            "quorum": { "threshold": 2, "window": 30, "require_grounded": true },
            "weights": { "recency": 5, "importance": 30, "relevance": 20, "half_life": 20 },
        }),
    );
    assert_eq!(
        serde_json::from_value::<EpisodePolicy>(value).expect("deserializes"),
        EpisodePolicy::DEFAULT,
    );

    let value = serde_json::to_value(state()).expect("serializes");
    assert_eq!(
        value,
        serde_json::json!({
            "conversation": {
                "desk_id": "engineering",
                "desk_name": "Engineering",
                "thread_root": null,
            },
            "spent": 0,
            "phase": "deliberate",
            "thresholds": [],
            "watermark": 0,
        }),
    );
    assert_eq!(
        serde_json::from_value::<EpisodeState>(value).expect("deserializes"),
        state(),
    );
}

#[test]
fn the_default_policy_bounds_the_episode() {
    // A finite budget is what makes termination a property rather than a hope.
    assert_eq!(EpisodePolicy::default(), EpisodePolicy::DEFAULT);
    assert_eq!(EpisodePolicy::default().turn_budget, 12);
}

#[test]
fn every_step_pins_its_tagged_wire_form() {
    assert_eq!(
        serde_json::to_value(HiveStep::Idle).expect("serializes"),
        serde_json::json!({ "step": "idle" }),
    );
    assert_eq!(
        serde_json::to_value(HiveStep::Exhausted { spent: 12 }).expect("serializes"),
        serde_json::json!({ "step": "exhausted", "spent": 12 }),
    );
    assert_eq!(
        serde_json::to_value(HiveStep::Deadlocked {
            topics: vec!["stage".into(), "ship".into()],
        })
        .expect("serializes"),
        serde_json::json!({ "step": "deadlocked", "topics": ["stage", "ship"] }),
    );
}

// --- The invariant ---

#[test]
fn a_speaking_step_authorizes_exactly_one_turn() {
    let room = Room::new();
    let turn = speaking(run(&room, &state(), &converging(), &EpisodePolicy::DEFAULT));
    assert!(MEMBERS.contains(&turn.agent_id.as_str()));
    assert_eq!(turn.next_state.spent, 1);
}

// --- Termination ---

#[test]
fn a_spent_budget_is_exhausted_and_authorizes_no_turn() {
    let room = Room::new();
    let policy = EpisodePolicy {
        turn_budget: 4,
        ..EpisodePolicy::DEFAULT
    };
    let spent = EpisodeState {
        spent: 4,
        ..state()
    };
    assert_eq!(
        run(&room, &spent, &converging(), &policy),
        HiveStep::Exhausted { spent: 4 },
    );
}

#[test]
fn a_zero_budget_never_authorizes_a_first_turn() {
    let room = Room::new();
    let policy = EpisodePolicy {
        turn_budget: 0,
        ..EpisodePolicy::DEFAULT
    };
    assert_eq!(
        run(&room, &state(), &converging(), &policy),
        HiveStep::Exhausted { spent: 0 },
    );
}

#[test]
fn an_episode_terminates_within_its_budget() {
    let room = Room::new();
    let policy = EpisodePolicy {
        turn_budget: 5,
        ..EpisodePolicy::DEFAULT
    };
    let mut state = state();
    // One proposal only: below quorum, so the room keeps deliberating and the
    // budget is what stops it rather than a decision.
    let transcript = [
        operator(1, "Decide how to roll this out."),
        said(2, "planner", "!propose #stage Stage the rollout."),
    ];
    // Every step either terminates or strictly advances the spend, so the loop
    // cannot run past the budget.
    for expected in 1..=policy.turn_budget {
        let turn = speaking(run(&room, &state, &transcript, &policy));
        assert_eq!(turn.next_state.spent, expected);
        state = turn.next_state;
    }
    assert_eq!(
        run(&room, &state, &transcript, &policy),
        HiveStep::Exhausted { spent: 5 },
    );
}

#[test]
fn nobody_speaks_when_every_threshold_is_unreachable() {
    let room = Room::new();
    let state = EpisodeState {
        thresholds: MEMBERS
            .iter()
            .map(|id| AgentThreshold::new(*id, i64::MAX))
            .collect(),
        ..state()
    };
    assert_eq!(
        run(&room, &state, &converging(), &EpisodePolicy::DEFAULT),
        HiveStep::Idle,
    );
}

// --- Quorum, the one-way phase change, and convergence ---

#[test]
fn quorum_flips_the_phase_once_and_then_converges() {
    let room = Room::new();
    let policy = EpisodePolicy::DEFAULT;

    // Deliberating with quorum reached: one commit turn is authorized.
    let turn = speaking(run(&room, &state(), &converging(), &policy));
    assert_eq!(turn.phase, Phase::Commit);
    assert_eq!(turn.next_state.phase, Phase::Commit);

    // The commit-phase turn speaks, but records nothing: phase alone must
    // not be read as proof that the room recorded its decision.
    let mut transcript = converging();
    assert!(
        matches!(
            run(&room, &turn.next_state, &transcript, &policy),
            HiveStep::Speak { .. },
        ),
        "a commit-phase turn that recorded no `!commit` must not converge",
    );

    // Once the authorized speaker actually commits the carried topic, the
    // episode reports its decision.
    transcript.push(said(4, &turn.agent_id, "!commit #stage Locking this in."));
    let HiveStep::Converged { topic, standing } =
        run(&room, &turn.next_state, &transcript, &policy)
    else {
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
    let turn = speaking(run(&room, &state(), &transcript, &EpisodePolicy::DEFAULT));
    assert_eq!(
        turn.phase,
        Phase::Deliberate,
        "non-member traces must not carry a topic to quorum",
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
        ..EpisodePolicy::DEFAULT
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

// --- Deadlock, and cross-inhibition through the whole machine ---

#[test]
fn a_deadlock_a_dissenter_can_still_break_authorizes_one_more_turn() {
    let mut room = Room::new();
    // A fourth member who has backed neither side is free to break the tie.
    room.members.push(member("archivist"));
    room.desks[0].members.push("archivist".into());

    let step = run(&room, &state(), &deadlocked(), &EpisodePolicy::DEFAULT);
    assert!(
        matches!(step, HiveStep::Speak { .. }),
        "while a member who has backed neither side exists, the room is not \
         deadlocked; it gets another turn — got {step:?}",
    );

    // The same room without that member is terminal, so the dissenter is what
    // makes the difference rather than anything else in the transcript.
    let committed = Room::new();
    assert_eq!(
        run(&committed, &state(), &deadlocked(), &EpisodePolicy::DEFAULT),
        HiveStep::Deadlocked {
            topics: vec![TopicId("stage".into()), TopicId("ship".into())],
        },
    );
}

#[test]
fn addressed_precedence_does_not_mask_an_available_dissenter() {
    // archivist backs neither tied topic, so the room must stay open — but
    // archivist's own message is also targeted by a later trace, which would
    // classify archivist's bid as `Addressed` rather than `Dissent`. The
    // terminal check must see the dissent structurally, from the standings,
    // rather than through that bid-reason precedence.
    let mut room = Room::new();
    room.members.push(member("archivist"));
    room.desks[0].members.push("archivist".into());

    let mut transcript = deadlocked();
    transcript.push(said(5, "archivist", "!question What about latency?"));
    transcript.push(said(6, "critic", "!object >5 Out of scope."));

    let step = run(&room, &state(), &transcript, &EpisodePolicy::DEFAULT);
    assert!(
        matches!(step, HiveStep::Speak { .. }),
        "archivist is free to break the tie even though addressed, got {step:?}",
    );
}

#[test]
fn a_deadlock_nobody_can_break_is_terminal() {
    // In `deadlocked()` every member has taken a side: planner backs both,
    // critic backs `stage`, scout backs `ship`. Nobody is left to break it.
    let room = Room::new();
    assert_eq!(
        run(&room, &state(), &deadlocked(), &EpisodePolicy::DEFAULT),
        HiveStep::Deadlocked {
            topics: vec![TopicId("stage".into()), TopicId("ship".into())],
        },
    );
}

#[test]
fn a_grounded_objection_carries_the_room_through_a_deadlock() {
    let mut room = Room::new();
    room.desks[0].members = vec!["planner".into(), "scout".into(), "critic".into()];
    let mut transcript = deadlocked();
    transcript.push(said(5, "critic", "!object >4 ^3 That precedent differs."));

    let HiveStep::Speak { turn } = run(&room, &state(), &transcript, &EpisodePolicy::DEFAULT)
    else {
        panic!("expected the room to move to commit")
    };
    assert_eq!(turn.phase, Phase::Commit);

    transcript.push(said(6, &turn.agent_id, "!commit #stage Locking this in."));
    let HiveStep::Converged { topic, .. } = run(
        &room,
        &turn.next_state,
        &transcript,
        &EpisodePolicy::DEFAULT,
    ) else {
        panic!("expected convergence")
    };
    assert_eq!(topic, TopicId("stage".into()));
}

// --- The watermark ---

#[test]
fn traces_at_or_below_the_watermark_are_context_not_votes() {
    let room = Room::new();
    let opened_late = EpisodeState::opened(conversation(), Sequence(3));
    // The whole converging exchange sits at or below the watermark.
    let step = run(&room, &opened_late, &converging(), &EpisodePolicy::DEFAULT);
    let turn = speaking(step);
    assert_eq!(
        turn.phase,
        Phase::Deliberate,
        "an episode must not inherit the quorum of the conversation before it",
    );
}

// --- Blind visibility ---

#[test]
fn the_opening_round_is_blind_until_every_member_has_been_heard() {
    let room = Room::new();
    let policy = EpisodePolicy::DEFAULT;

    let early = speaking(run(&room, &state(), &converging(), &policy));
    assert_eq!(early.visibility, Visibility::Blind);

    let mut heard = converging();
    heard.push(said(4, "scout", "!question"));
    let late = speaking(run(&room, &state(), &heard, &policy));
    assert_eq!(late.visibility, Visibility::Full);
}

#[test]
fn a_disabled_blind_round_is_always_full() {
    let room = Room::new();
    let policy = EpisodePolicy {
        blind_round: false,
        ..EpisodePolicy::DEFAULT
    };
    let turn = speaking(run(&room, &state(), &converging(), &policy));
    assert_eq!(turn.visibility, Visibility::Full);
}

#[test]
fn a_blind_turn_hides_peers_but_keeps_the_task_and_its_own_work() {
    let transcript = [
        operator(1, "Decide how to roll this out."),
        said(2, "planner", "!propose #stage"),
        said(3, "critic", "!propose #ship"),
        SessionMessage {
            sequence: Sequence(4),
            author: SessionAuthor::System {
                kind: "workflow".into(),
                label: "CI".into(),
            },
            content: "build green".into(),
        },
    ];
    let turn = HiveTurn {
        agent_id: "planner".into(),
        phase: Phase::Deliberate,
        visibility: Visibility::Blind,
        reason: BidReason::Salience,
        next_state: state(),
    };

    let blind = project_for(&turn, &transcript);
    assert_eq!(
        blind.iter().map(|m| m.sequence.0).collect::<Vec<_>>(),
        [1, 2, 4],
        "a peer's position is withheld; the task, the system notice and its own work are not",
    );

    let revealed = HiveTurn {
        visibility: Visibility::Full,
        ..turn
    };
    assert_eq!(project_for(&revealed, &transcript).len(), transcript.len());
}

#[test]
fn a_blind_turn_preserves_pre_episode_agent_context() {
    // The watermark sits at sequence 1: everything at or below it is the
    // conversation the episode opened on top of, not a peer position formed
    // within this episode, so it must survive a blind projection.
    let transcript = [
        said(1, "planner", "Already found the culprit function."),
        said(2, "planner", "!propose #stage"),
        said(3, "critic", "!propose #ship"),
    ];
    let turn = HiveTurn {
        agent_id: "critic".into(),
        phase: Phase::Deliberate,
        visibility: Visibility::Blind,
        reason: BidReason::Salience,
        next_state: EpisodeState::opened(conversation(), Sequence(1)),
    };

    let blind = project_for(&turn, &transcript);
    assert_eq!(
        blind.iter().map(|m| m.sequence.0).collect::<Vec<_>>(),
        [1, 3],
        "the pre-episode message at the watermark remains visible even though \
         a peer authored it; only the later peer proposal formed within the \
         episode is hidden",
    );
}

// --- Thresholds carried across turns ---

#[test]
fn speaking_costs_the_speaker_and_silence_accrues_standing() {
    let room = Room::new();
    let turn = speaking(run(&room, &state(), &converging(), &EpisodePolicy::DEFAULT));
    let speaker = turn.agent_id.clone();

    let charged = turn.next_state.thresholds;
    assert_eq!(charged.len(), MEMBERS.len());
    let spoke = charged
        .iter()
        .find(|held| held.agent_id == speaker)
        .expect("the speaker is charged");
    assert!(spoke.threshold > 0);
    assert!(
        charged
            .iter()
            .filter(|held| held.agent_id != speaker)
            .all(|held| held.threshold < 0),
        "members who stayed silent must get cheaper to reach",
    );
}

// --- Failure paths ---

#[test]
fn a_malformed_roster_or_desk_snapshot_is_rejected() {
    let mut room = Room::new();
    room.members.push(member("planner"));
    let error = step(
        &state(),
        &converging(),
        &room.roster(),
        &room.desk_set(),
        &EpisodePolicy::DEFAULT,
    )
    .expect_err("duplicate roster member");
    assert_eq!(error.to_string(), "duplicate roster member id `planner`");

    let mut room = Room::new();
    room.desks.push(desks().remove(0));
    let error = step(
        &state(),
        &converging(),
        &room.roster(),
        &room.desk_set(),
        &EpisodePolicy::DEFAULT,
    )
    .expect_err("duplicate desk");
    assert_eq!(error.to_string(), "duplicate desk id `engineering`");
}

#[test]
fn an_unknown_desk_is_rejected() {
    let room = Room::new();
    let elsewhere = EpisodeState::opened(
        Conversation {
            desk_id: "design".into(),
            desk_name: "Design".into(),
            thread_root: None,
        },
        Sequence(0),
    );
    let error = step(
        &elsewhere,
        &converging(),
        &room.roster(),
        &room.desk_set(),
        &EpisodePolicy::DEFAULT,
    )
    .expect_err("unknown desk");
    assert_eq!(error.to_string(), "unknown desk `design`");
}

#[test]
fn a_threshold_naming_a_non_member_is_rejected() {
    let room = Room::new();
    let state = EpisodeState {
        thresholds: vec![AgentThreshold::new("stranger", 0)],
        ..state()
    };
    let error = step(
        &state,
        &converging(),
        &room.roster(),
        &room.desk_set(),
        &EpisodePolicy::DEFAULT,
    )
    .expect_err("unknown threshold member");
    assert_eq!(
        error.to_string(),
        "threshold `stranger` is not an active member of desk `engineering`",
    );
}

#[test]
fn a_retired_member_neither_bids_nor_holds_a_threshold() {
    let mut room = Room::new();
    room.retired = vec!["scout".into()];
    let turn = speaking(run(&room, &state(), &converging(), &EpisodePolicy::DEFAULT));
    assert_ne!(turn.agent_id, "scout");
    assert!(
        turn.next_state
            .thresholds
            .iter()
            .all(|held| held.agent_id != "scout"),
    );
}

#[test]
fn a_malformed_policy_surfaces_from_the_quorum_fold() {
    let room = Room::new();
    let policy = EpisodePolicy {
        quorum: crate::quorum::QuorumPolicy {
            threshold: 0,
            ..crate::quorum::QuorumPolicy::DEFAULT
        },
        ..EpisodePolicy::DEFAULT
    };
    let error = step(
        &state(),
        &converging(),
        &room.roster(),
        &room.desk_set(),
        &policy,
    )
    .expect_err("zero threshold");
    assert_eq!(error.to_string(), "quorum threshold must not be zero");
}

#[test]
fn the_budget_check_bounds_the_spend_before_it_can_overflow() {
    let room = Room::new();
    let policy = EpisodePolicy {
        turn_budget: u32::MAX,
        ..EpisodePolicy::DEFAULT
    };
    // One below the ceiling still advances, landing exactly on it...
    let brimming = EpisodeState {
        spent: u32::MAX - 1,
        ..state()
    };
    let turn = speaking(run(&room, &brimming, &converging(), &policy));
    assert_eq!(turn.next_state.spent, u32::MAX);

    // ...and at the ceiling the budget check fires first, so the addition is
    // never reached. That is why there is no overflow error to return.
    assert_eq!(
        run(&room, &turn.next_state, &converging(), &policy),
        HiveStep::Exhausted { spent: u32::MAX },
    );
}
