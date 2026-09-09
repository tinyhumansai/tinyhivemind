//! Per-turn attention dynamics carried across an episode: blind visibility
//! during the opening round, and the threshold charge that rotates the floor
//! by making the speaker costlier and everyone else cheaper to reach.

use super::super::*;
use super::support::{
    MEMBERS, Room, converging, operator, run, said, sequential, speaking, spoke, state,
};
use crate::attention::BidReason;
use tinyhivemind::Sequence;
use tinyhivemind::aside::Audience;

#[test]
fn the_opening_round_is_blind_until_every_member_has_been_heard() {
    let room = Room::new();
    let policy = sequential();

    let early = speaking(run(&room, &state(), &converging(), &policy));
    assert_eq!(early.visibility, Visibility::Blind);

    let mut heard = converging();
    heard.push(said(4, "scout", "!question"));
    let late = speaking(run(&room, &state(), &heard, &policy));
    assert_eq!(late.visibility, Visibility::Full);
}

#[test]
fn a_marker_less_turn_still_counts_toward_ending_the_blind_round() {
    // Blindness tracks who has *taken a turn*, not who has cast a vote. A
    // member that speaks plain prose with no `!marker` deposits no trace, but
    // must still count as heard -- otherwise a member who never has anything
    // to formally propose keeps the whole room blind forever.
    let room = Room::new();
    let policy = sequential();

    let mut heard = converging();
    heard.push(said(4, "scout", "Just thinking out loud, no vote yet."));
    let late = speaking(run(&room, &state(), &heard, &policy));
    assert_eq!(
        late.visibility,
        Visibility::Full,
        "a member who authored a turn with no marker must still count as heard",
    );
}

#[test]
fn a_disabled_blind_round_is_always_full() {
    let room = Room::new();
    let policy = EpisodePolicy {
        blind_round: false,
        ..sequential()
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
            audience: Audience::Desk,
            elided: None,
        },
    ];
    let turn = HiveTurn {
        agent_id: "planner".into(),
        phase: Phase::Deliberate,
        visibility: Visibility::Blind,
        reason: BidReason::Salience,
        watermark: state().watermark,
        // The round was folded at the newest row, which is what `step` always
        // sets: nothing is concurrent with this turn, so the round boundary
        // withholds nothing and only `Visibility` is under test here.
        round_start: Sequence(4),
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
        watermark: Sequence(1),
        round_start: Sequence(3),
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

#[test]
fn a_continuing_wide_blind_round_selects_the_unheard_member_over_a_louder_heard_one() {
    // `unheard` bounded a *wide* blind round's width, but `floor_round` still
    // ranked bids from every member -- heard and unheard alike. A member
    // already heard this episode can out-bid an unheard one (here, `planner`
    // is addressed by `critic`'s citation and picks up `ADDRESSED_BONUS`),
    // and a round continuing the blind phase would reselect that heard
    // member instead of the one still owed a turn: budget spent, the blind
    // phase no closer to closing, potentially all the way to exhaustion.
    //
    // `scout` has not spoken. `planner` and `critic` both have, and
    // `planner`'s citation bonus dwarfs anything `scout` can bid at zero
    // threshold, so this pins the fix: a concurrent round is filtered to
    // unheard identities, not merely capped at their count.
    //
    // `round_width: 2` matters here, not `sequential`'s `1` -- the fix is
    // scoped to a genuinely concurrent round precisely so that `round_width:
    // 1` keeps reproducing the sequential episode bit for bit, floor rotation
    // left to `charged`'s threshold dynamics exactly as it always was.
    let room = Room::new();
    let policy = EpisodePolicy {
        round_width: 2,
        revealed_width: 2,
        ..EpisodePolicy::DEFAULT
    };
    let transcript = vec![
        said(1, "planner", "!propose #stage"),
        said(2, "critic", "!support #stage ^1"),
    ];

    let (turns, _) = round(run(&room, &state(), &transcript, &policy));
    assert_eq!(
        turns.iter().map(|turn| turn.agent_id.as_str()).collect::<Vec<_>>(),
        vec!["scout"],
        "the still-blind round must pick the one member left unheard, \
         not the louder bid from a member already heard",
    );
    assert_eq!(turns[0].visibility, Visibility::Blind);
}

#[test]
fn speaking_costs_the_speaker_and_silence_accrues_standing() {
    let room = Room::new();
    let (turn, next) = spoke(run(&room, &state(), &converging(), &sequential()));
    let speaker = turn.agent_id.clone();

    let charged = next.thresholds;
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
