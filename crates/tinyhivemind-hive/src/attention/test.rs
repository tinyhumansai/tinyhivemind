//! Unit tests for the attention market.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;

use crate::{
    directory::{Directory, DirectoryPolicy, directory},
    quorum::{QuorumPolicy, standings},
    salience::SalienceWeights,
    trace::read,
};
use tinyhivemind::{Sequence, SessionAuthor, SessionMessage};

const MEMBERS: [&str; 3] = ["planner", "critic", "scout"];

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

struct Fixture {
    traces: Vec<Trace>,
    standings: Vec<TopicStanding>,
    at: Sequence,
}

fn fixture(transcript: &[SessionMessage]) -> Fixture {
    let at = transcript.last().map_or(Sequence(0), |m| m.sequence);
    let traces = read(transcript);
    let standings = standings(&traces, at, &QuorumPolicy::DEFAULT).expect("folds");
    Fixture {
        traces,
        standings,
        at,
    }
}

const QUORUM: QuorumPolicy = QuorumPolicy::DEFAULT;

fn context<'a>(
    fixture: &'a Fixture,
    members: &'a [&'a str],
    thresholds: &'a [AgentThreshold],
    weights: &'a SalienceWeights,
) -> BidContext<'a> {
    BidContext {
        traces: &fixture.traces,
        standings: &fixture.standings,
        members,
        thresholds,
        at: fixture.at,
        weights,
        dominance_cap: 50,
        repetition_cap: 3,
        quorum: &QUORUM,
        directory: None,
        directory_policy: None,
        defer_cap: None,
    }
}

fn bid_for<'a>(bids: &'a [Bid], agent: &str) -> &'a Bid {
    bids.iter()
        .find(|bid| bid.agent_id == agent)
        .unwrap_or_else(|| panic!("no bid from {agent}"))
}

#[test]
fn a_threshold_and_bid_pin_their_wire_forms() {
    let threshold = AgentThreshold {
        agent_id: "planner".into(),
        threshold: 250,
        affinity: vec![("stage".into(), 90)],
    };
    let value = serde_json::to_value(&threshold).expect("serializes");
    assert_eq!(
        value,
        serde_json::json!({
            "agent_id": "planner",
            "threshold": 250,
            "affinity": [["stage", 90]],
        }),
    );
    assert_eq!(
        serde_json::from_value::<AgentThreshold>(value).expect("deserializes"),
        threshold,
    );

    let bid = Bid {
        agent_id: "planner".into(),
        urge: 1_200,
        reason: BidReason::Dissent,
    };
    let value = serde_json::to_value(&bid).expect("serializes");
    assert_eq!(
        value,
        serde_json::json!({ "agent_id": "planner", "urge": 1_200, "reason": "dissent" }),
    );
    assert_eq!(
        serde_json::from_value::<Bid>(value).expect("deserializes"),
        bid
    );
}

#[test]
fn every_bid_reason_pins_its_wire_spelling() {
    for (reason, spelling) in [
        (BidReason::Addressed, "addressed"),
        (BidReason::Dissent, "dissent"),
        (BidReason::Knows, "knows"),
        (BidReason::Quiet, "quiet"),
        (BidReason::Salience, "salience"),
    ] {
        assert_eq!(
            serde_json::to_value(reason).expect("serializes"),
            serde_json::json!(spelling),
        );
    }
}

#[test]
fn an_undeclared_affinity_is_neutral_rather_than_uninterested() {
    let threshold = AgentThreshold::new("planner", 0);
    assert_eq!(threshold.relevance(None), 50);
    assert_eq!(threshold.relevance(Some(&"stage".into())), 50);

    let declared = AgentThreshold {
        affinity: vec![("stage".into(), 95)],
        ..AgentThreshold::new("planner", 0)
    };
    assert_eq!(declared.relevance(Some(&"stage".into())), 95);
    assert_eq!(declared.relevance(Some(&"ship".into())), 50);
    assert_eq!(declared.relevance(None), 50);
}

#[test]
fn a_declared_relevance_separates_an_undeclared_topic_from_a_neutral_one() {
    let declared = AgentThreshold {
        affinity: vec![("stage".into(), 95), ("ship".into(), 0)],
        ..AgentThreshold::new("planner", 0)
    };
    assert_eq!(declared.declared_relevance(&"stage".into()), Some(95));
    // Declared as *irrelevant* is a claim; it must not read back as neutral.
    assert_eq!(declared.declared_relevance(&"ship".into()), Some(0));
    assert_eq!(declared.declared_relevance(&"pool".into()), None);
    assert_eq!(declared.relevance(Some(&"pool".into())), 50);
    assert_eq!(
        AgentThreshold::new("scout", 0).declared_relevance(&"pool".into()),
        None
    );
}

#[test]
fn a_duplicate_threshold_record_is_rejected() {
    let fixture = fixture(&[said(1, "planner", "!propose #stage")]);
    let thresholds = [
        AgentThreshold::new("planner", 0),
        AgentThreshold::new("planner", 5),
    ];
    let weights = SalienceWeights::DEFAULT;
    let error =
        bids(&context(&fixture, &MEMBERS, &thresholds, &weights)).expect_err("duplicate threshold");
    assert_eq!(error.to_string(), "duplicate agent threshold `planner`");
}

#[test]
fn a_malformed_weight_surfaces_from_the_salience_fold() {
    let fixture = fixture(&[said(1, "planner", "!propose #stage")]);
    let weights = SalienceWeights {
        half_life: 0,
        ..SalienceWeights::DEFAULT
    };
    let error =
        bids(&context(&fixture, &MEMBERS, &[], &weights)).expect_err("zero half life propagates");
    assert_eq!(error.to_string(), "salience half life must not be zero");
}

#[test]
fn an_empty_medium_still_lets_every_member_bid() {
    let fixture = fixture(&[]);
    let weights = SalienceWeights::DEFAULT;
    let bids = bids(&context(&fixture, &MEMBERS, &[], &weights)).expect("bids");
    assert_eq!(bids.len(), MEMBERS.len());
    assert!(bids.iter().all(|bid| bid.reason == BidReason::Salience));
}

#[test]
fn a_member_whose_urge_misses_its_threshold_does_not_bid() {
    let fixture = fixture(&[said(1, "planner", "!propose #stage")]);
    let thresholds = [AgentThreshold::new("scout", i64::MAX)];
    let weights = SalienceWeights::DEFAULT;
    let bids = bids(&context(&fixture, &MEMBERS, &thresholds, &weights)).expect("bids");
    assert_eq!(bids.len(), 2);
    assert!(bids.iter().all(|bid| bid.agent_id != "scout"));
}

#[test]
fn nobody_bids_when_every_threshold_is_unreachable() {
    let fixture = fixture(&[said(1, "planner", "!propose #stage")]);
    let thresholds: Vec<AgentThreshold> = MEMBERS
        .iter()
        .map(|member| AgentThreshold::new(*member, i64::MAX))
        .collect();
    let weights = SalienceWeights::DEFAULT;
    let bids = bids(&context(&fixture, &MEMBERS, &thresholds, &weights)).expect("bids");
    assert!(bids.is_empty());
    assert!(floor_holder(&bids).is_none());
}

#[test]
fn a_higher_affinity_member_outbids_a_neutral_one() {
    let fixture = fixture(&[said(1, "planner", "!propose #stage")]);
    let thresholds = [AgentThreshold {
        affinity: vec![("stage".into(), 100)],
        ..AgentThreshold::new("scout", 0)
    }];
    let weights = SalienceWeights::DEFAULT;
    let bids = bids(&context(&fixture, &MEMBERS, &thresholds, &weights)).expect("bids");
    assert!(bid_for(&bids, "scout").urge > bid_for(&bids, "critic").urge);
}

#[test]
fn a_member_whose_message_was_cited_or_objected_to_bids_addressed() {
    let cited = fixture(&[
        said(1, "planner", "!propose #stage"),
        said(2, "critic", "!support #stage ^1"),
    ]);
    let weights = SalienceWeights::DEFAULT;
    let cited_bids = bids(&context(&cited, &MEMBERS, &[], &weights)).expect("bids");
    assert_eq!(bid_for(&cited_bids, "planner").reason, BidReason::Addressed);
    assert_eq!(bid_for(&cited_bids, "scout").reason, BidReason::Salience);

    let objected = fixture(&[
        said(1, "planner", "!propose #stage"),
        said(2, "critic", "!object >1 ^0"),
    ]);
    let objected_bids = bids(&context(&objected, &MEMBERS, &[], &weights)).expect("bids");
    assert_eq!(
        bid_for(&objected_bids, "planner").reason,
        BidReason::Addressed
    );
}

#[test]
fn citing_your_own_message_does_not_address_you() {
    let fixture = fixture(&[
        said(1, "planner", "!propose #stage"),
        said(2, "planner", "!evidence ^1"),
    ]);
    let weights = SalienceWeights::DEFAULT;
    let bids = bids(&context(&fixture, &MEMBERS, &[], &weights)).expect("bids");
    assert_ne!(bid_for(&bids, "planner").reason, BidReason::Addressed);
}

#[test]
fn a_member_backing_neither_deadlocked_side_bids_dissent() {
    let fixture = fixture(&[
        said(1, "planner", "!propose #stage"),
        said(2, "critic", "!propose #ship"),
    ]);
    let weights = SalienceWeights::DEFAULT;
    let bids = bids(&context(&fixture, &MEMBERS, &[], &weights)).expect("bids");
    assert_eq!(bid_for(&bids, "scout").reason, BidReason::Dissent);
    // The two advocates have taken sides, so neither can break their own tie.
    assert_ne!(bid_for(&bids, "planner").reason, BidReason::Dissent);
    assert_ne!(bid_for(&bids, "critic").reason, BidReason::Dissent);
    assert_eq!(floor_holder(&bids).expect("a winner").agent_id, "scout");
}

#[test]
fn one_clear_leader_is_not_a_deadlock_and_raises_no_dissent() {
    let fixture = fixture(&[
        said(1, "planner", "!propose #stage"),
        said(2, "critic", "!support #stage ^1"),
    ]);
    let weights = SalienceWeights::DEFAULT;
    let bids = bids(&context(&fixture, &MEMBERS, &[], &weights)).expect("bids");
    assert!(bids.iter().all(|bid| bid.reason != BidReason::Dissent));
}

#[test]
fn a_dominant_speaker_is_damped_and_the_quietest_member_is_lifted() {
    // planner carries every surviving grounded contribution.
    let transcript = [
        said(1, "planner", "!propose #stage"),
        said(2, "planner", "!support #stage ^1"),
        said(3, "planner", "!evidence #stage ^1"),
        said(4, "planner", "!support #stage ^1 again"),
        said(5, "critic", "!support #stage ^1"),
    ];
    let fixture = fixture(&transcript);
    let weights = SalienceWeights::DEFAULT;
    let mut context = context(&fixture, &MEMBERS, &[], &weights);
    context.dominance_cap = 50;

    let damped = bids(&context).expect("bids");
    assert_eq!(bid_for(&damped, "scout").reason, BidReason::Quiet);

    // The same room with the guard effectively disabled leaves planner alone.
    context.dominance_cap = 100;
    let undamped = bids(&context).expect("bids");
    assert!(undamped.iter().all(|bid| bid.reason != BidReason::Quiet));
    assert!(
        bid_for(&undamped, "planner").urge > bid_for(&damped, "planner").urge,
        "the dominance guard must actually cost the dominant speaker urge",
    );
}

#[test]
fn share_is_measured_over_grounded_surviving_contributions_not_message_count() {
    // scout emits the most messages by far, but none are grounded, so its
    // share stays zero and it is still the member the guard reaches for.
    let transcript = [
        said(1, "planner", "!propose #stage"),
        said(2, "planner", "!support #stage ^1"),
        said(3, "planner", "!evidence #stage ^1"),
        said(4, "planner", "!support #stage ^1"),
        said(5, "critic", "!support #stage ^1"),
        said(6, "scout", "!question"),
        said(7, "scout", "!question"),
        said(8, "scout", "!question"),
        said(9, "scout", "!question"),
    ];
    let fixture = fixture(&transcript);
    let weights = SalienceWeights::DEFAULT;
    let bids = bids(&context(&fixture, &MEMBERS, &[], &weights)).expect("bids");
    assert_eq!(
        bid_for(&bids, "scout").reason,
        BidReason::Quiet,
        "an agent cannot buy its way out of the equality guard with ungrounded chatter",
    );
}

#[test]
fn a_topic_at_the_repetition_cap_stops_paying_for_restatement() {
    let transcript = [
        said(1, "planner", "!propose #stage"),
        said(2, "critic", "!support #stage ^1"),
        said(3, "scout", "!support #stage ^1"),
    ];
    let fixture = fixture(&transcript);
    let weights = SalienceWeights::DEFAULT;
    let mut context = context(&fixture, &MEMBERS, &[], &weights);

    context.repetition_cap = 3;
    let saturated = bids(&context).expect("bids");
    context.repetition_cap = 0;
    let uncapped = bids(&context).expect("bids");

    assert!(
        bid_for(&uncapped, "critic").urge > bid_for(&saturated, "critic").urge,
        "support on a topic three peers already back must stop scoring",
    );
}

#[test]
fn the_floor_holder_is_the_argmax_and_ties_break_by_desk_order() {
    let bids = [
        Bid {
            agent_id: "planner".into(),
            urge: 500,
            reason: BidReason::Salience,
        },
        Bid {
            agent_id: "critic".into(),
            urge: 900,
            reason: BidReason::Salience,
        },
        Bid {
            agent_id: "scout".into(),
            urge: 900,
            reason: BidReason::Salience,
        },
    ];
    assert_eq!(floor_holder(&bids).expect("a winner").agent_id, "critic");
    assert!(floor_holder(&[]).is_none());
}

// --- Delegation ---

/// The directory policy the delegation tests read, with a window wide enough
/// to hold the whole fixture transcript.
fn directory_policy() -> DirectoryPolicy {
    DirectoryPolicy {
        window: 100,
        ..DirectoryPolicy::DEFAULT
    }
}

fn folded(fixture: &Fixture) -> Directory {
    directory(&fixture.traces, fixture.at, &directory_policy(), &[]).expect("folds")
}

/// Attach a folded directory to a bid context.
///
/// Written as a macro rather than a helper because the directory and the
/// policy have to outlive the borrow the context takes of them.
macro_rules! delegating {
    ($fixture:expr, $known:ident, $policy:ident, $context:ident) => {
        let $known = folded(&$fixture);
        let $policy = directory_policy();
        let weights = SalienceWeights::DEFAULT;
        let mut $context = context(&$fixture, &MEMBERS, &[], &weights);
        $context.directory = Some(&$known);
        $context.directory_policy = Some(&$policy);
    };
}

/// A room arguing an ungrounded decoy while the scout's fact goes unheard.
///
/// The critic's opinion cites nothing, so under `require_grounded` it is not
/// support and `#retries` stays one supporter short of carrying. The scout has
/// put the fact that settles it on the floor and taken no position at all.
fn hidden_profile() -> Fixture {
    fixture(&[
        said(1, "planner", "!propose #retries A retry storm explains it."),
        said(2, "critic", "!support #retries The graph agrees."),
        said(
            3,
            "scout",
            "!evidence #retries The retry flag has been off for a week.",
        ),
    ])
}

#[test]
fn knows_gives_the_floor_to_the_holder_of_an_uncited_fact() {
    let fixture = hidden_profile();
    delegating!(fixture, known, policy, context);
    assert_eq!(
        known
            .top(&"retries".into())
            .map(|held| held.agent_id.as_str()),
        Some("scout")
    );

    let bids = bids(&context).expect("bids");
    assert_eq!(bid_for(&bids, "scout").reason, BidReason::Knows);
    assert_eq!(floor_holder(&bids).expect("a winner").agent_id, "scout");
}

#[test]
fn knows_never_fires_without_a_directory() {
    let fixture = hidden_profile();
    let weights = SalienceWeights::DEFAULT;
    // The same room with no directory in the context at all.
    let blind = bids(&context(&fixture, &MEMBERS, &[], &weights)).expect("bids");
    assert!(blind.iter().all(|bid| bid.reason != BidReason::Knows));

    // And with a directory but no policy to read it by: the pair is matched.
    let known = folded(&fixture);
    let mut half = context(&fixture, &MEMBERS, &[], &weights);
    half.directory = Some(&known);
    let half = bids(&half).expect("bids");
    assert!(half.iter().all(|bid| bid.reason != BidReason::Knows));
}

#[test]
fn knows_stops_once_the_member_has_taken_a_position_on_the_topic() {
    let fixture = fixture(&[
        said(1, "planner", "!propose #retries A retry storm explains it."),
        said(2, "critic", "!support #retries The graph agrees."),
        said(
            3,
            "scout",
            "!evidence #retries The retry flag has been off for a week.",
        ),
        // The scout has now argued the topic rather than only grounded it.
        said(
            4,
            "scout",
            "!refute #retries ^3 So the storm is not the cause.",
        ),
    ]);
    delegating!(fixture, known, policy, context);
    let bids = bids(&context).expect("bids");
    assert!(
        bids.iter().all(|bid| bid.reason != BidReason::Knows),
        "the bonus buys an unheard fact its hearing and nothing after",
    );
}

#[test]
fn knows_never_fires_on_a_carried_topic() {
    // A grounded second supporter carries `#retries`, so it is settled rather
    // than contested and there is nothing left to route to.
    let fixture = fixture(&[
        said(1, "planner", "!propose #retries A retry storm explains it."),
        said(2, "critic", "!support #retries ^1 The graph agrees."),
        said(
            3,
            "scout",
            "!evidence #retries The retry flag has been off for a week.",
        ),
    ]);
    assert!(fixture.standings[0].carried(&QUORUM));
    delegating!(fixture, known, policy, context);
    let bids = bids(&context).expect("bids");
    assert!(bids.iter().all(|bid| bid.reason != BidReason::Knows));
}

#[test]
fn an_addressed_fact_holder_bids_addressed_rather_than_knows() {
    let fixture = fixture(&[
        said(1, "planner", "!propose #retries A retry storm explains it."),
        said(2, "critic", "!support #retries The graph agrees."),
        said(
            3,
            "scout",
            "!evidence #retries The retry flag has been off for a week.",
        ),
        said(4, "critic", "!question ^3 Off since when?"),
    ]);
    delegating!(fixture, known, policy, context);
    let bids = bids(&context).expect("bids");
    // A question aimed at the scout's message is a fact about *this* message,
    // where the directory is only an estimate.
    assert_eq!(bid_for(&bids, "scout").reason, BidReason::Addressed);
}

#[test]
fn knows_outranks_quiet_and_is_outranked_by_dissent() {
    assert!(BidReason::Addressed < BidReason::Dissent);
    assert!(BidReason::Dissent < BidReason::Knows);
    assert!(BidReason::Knows < BidReason::Quiet);
    assert!(BidReason::Quiet < BidReason::Salience);

    // Two proposals deadlock; the scout has backed neither and also tops the
    // directory. An unbroken deadlock ends the episode, so dissent wins.
    let fixture = fixture(&[
        said(1, "planner", "!propose #retries A retry storm explains it."),
        said(2, "critic", "!propose #stage Stage it instead."),
        said(
            3,
            "scout",
            "!evidence #retries The retry flag has been off for a week.",
        ),
    ]);
    delegating!(fixture, known, policy, context);
    assert_eq!(
        known
            .top(&"retries".into())
            .map(|held| held.agent_id.as_str()),
        Some("scout")
    );
    let bids = bids(&context).expect("bids");
    assert_eq!(bid_for(&bids, "scout").reason, BidReason::Dissent);
}

#[test]
fn a_deferral_promotes_its_topic_as_the_contested_one() {
    let fixture = fixture(&[
        said(1, "planner", "!propose #retries A retry storm explains it."),
        said(2, "critic", "!support #retries ^1 The graph agrees."),
        said(3, "scout", "!evidence #pool In-flight requests sit at 24."),
        // `#retries` has carried, so without the deferral no topic is
        // contested and the directory has nothing to route on.
        said(4, "critic", "!defer #pool Not my area."),
    ]);
    delegating!(fixture, known, policy, context);
    let promoted = bids(&context).expect("bids");
    assert_eq!(bid_for(&promoted, "scout").reason, BidReason::Knows);

    // At the cap the promotion stops, and the room falls back to a standing
    // that has already carried -- which is to say, to nothing.
    context.defer_cap = Some(1);
    let capped = bids(&context).expect("bids");
    assert!(capped.iter().all(|bid| bid.reason != BidReason::Knows));
}

#[test]
fn a_redelivered_deferral_counts_once_against_the_defer_cap() {
    let mut fixture = fixture(&[
        said(1, "planner", "!propose #retries A retry storm explains it."),
        said(2, "critic", "!support #retries ^1 The graph agrees."),
        said(3, "scout", "!evidence #pool In-flight requests sit at 24."),
        said(4, "critic", "!defer #pool Not my area."),
    ]);
    // The medium hands the same deferral over a second time, at the address it
    // already occupies. One deferral was authored, so one is what the cap sees.
    let redelivered = fixture
        .traces
        .last()
        .expect("the deferral was read")
        .clone();
    assert_eq!(redelivered.kind, TraceKind::Defer);
    fixture.traces.push(redelivered);

    delegating!(fixture, known, policy, context);
    context.defer_cap = Some(2);
    let bids = bids(&context).expect("bids");
    assert_eq!(bid_for(&bids, "scout").reason, BidReason::Knows);
}

#[test]
fn the_contested_topic_comes_from_the_highest_addressed_deferral() {
    let transcript = [
        said(1, "planner", "!propose #retries A retry storm explains it."),
        said(2, "critic", "!support #retries ^1 The graph agrees."),
        said(3, "scout", "!evidence #pool In-flight requests sit at 24."),
        said(4, "critic", "!defer #pool Not my area."),
        said(5, "planner", "!defer #cache Nor mine."),
    ];
    let cache: TopicId = "cache".into();

    let ordered = fixture(&transcript);
    {
        delegating!(ordered, known, policy, context);
        let live = distinct(context.traces);
        assert_eq!(contested_topic(&context, &live), Some(&cache));
    }

    // Redelivered out of order and doubled, the same transcript names the same
    // topic: the winner is the deferral with the highest address, not the one
    // the medium happened to hand over last.
    let mut shuffled = fixture(&transcript);
    shuffled.traces = shuffled
        .traces
        .iter()
        .rev()
        .chain(shuffled.traces.iter().rev())
        .cloned()
        .collect();
    {
        delegating!(shuffled, known, policy, context);
        let live = distinct(context.traces);
        assert_eq!(contested_topic(&context, &live), Some(&cache));
    }
}
