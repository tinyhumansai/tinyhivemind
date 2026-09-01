//! Unit tests for bounded cross-desk referral.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::{
    desk::{Desk, ResponderMode},
    dispatch::{
        DispatchKey, MentionDispatchDecision, MentionDispatchInput, MentionDispatchPolicy,
        mention_dispatch,
    },
    roster::RosterMember,
};

fn member(id: &str) -> RosterMember {
    RosterMember {
        id: id.to_owned(),
        name: Some(id.to_owned()),
    }
}

fn desk(id: &str, members: &[&str]) -> Desk {
    Desk {
        id: id.to_owned(),
        name: id.to_owned(),
        description: None,
        members: members.iter().map(|id| (*id).to_owned()).collect(),
        responder_mode: ResponderMode::Lead,
    }
}

fn members() -> Vec<RosterMember> {
    ["ada", "grace", "linus", "hedy"]
        .iter()
        .copied()
        .map(member)
        .collect()
}

fn desks() -> Vec<Desk> {
    vec![
        desk("payments", &["ada", "grace"]),
        desk("platform", &["linus", "hedy"]),
    ]
}

fn agent_mention(id: &str, offset: usize) -> Mention {
    Mention {
        target: MentionTarget::Agent { id: id.to_owned() },
        text: format!("@{id}"),
        offset,
        quiet: false,
    }
}

fn desk_mention(id: &str, offset: usize) -> Mention {
    Mention {
        target: MentionTarget::Desk { id: id.to_owned() },
        text: format!("@#{id}"),
        offset,
        quiet: false,
    }
}

fn conversation(desk_id: &str, thread_root: Option<u64>) -> DispatchConversation {
    DispatchConversation {
        desk_id: desk_id.to_owned(),
        thread_root,
    }
}

fn input(author: &str, desk_id: &str, mentions: Vec<Mention>) -> ReferralInput {
    ReferralInput {
        key: DispatchKey {
            trigger_sequence: 7,
        },
        conversation: conversation(desk_id, None),
        author_id: author.to_owned(),
        content: "body".to_owned(),
        mentions,
        hop: 0,
        origin: None,
    }
}

/// Everything on, two hops, which is what the swarm harness runs at.
const OPEN: ReferralPolicy = ReferralPolicy {
    enabled: true,
    max_hops: 2,
    reach: ReferralReach::Desks,
    returns: true,
};

fn decide(policy: ReferralPolicy, input: &ReferralInput) -> ReferralDecision {
    let members = members();
    let roster = Roster::new(&members, &[], &[]);
    let desk_records = desks();
    let desks = DeskSet::new(&desk_records, &[], &[], &[], &[]);
    referral(policy, input, &roster, &desks).expect("snapshots are well formed")
}

fn refused(policy: ReferralPolicy, input: &ReferralInput) -> NoReferralReason {
    match decide(policy, input) {
        ReferralDecision::None { reason } => reason,
        ReferralDecision::One { referral } => {
            panic!("expected no referral, got one to {}", referral.target_id)
        }
    }
}

fn accepted(policy: ReferralPolicy, input: &ReferralInput) -> Referral {
    match decide(policy, input) {
        ReferralDecision::One { referral } => *referral,
        ReferralDecision::None { reason } => panic!("expected a referral, got {reason:?}"),
    }
}

#[test]
fn a_default_policy_refers_nothing() {
    assert_eq!(
        refused(
            ReferralPolicy::DEFAULT,
            &input("ada", "payments", vec![agent_mention("linus", 0)])
        ),
        NoReferralReason::Disabled,
    );
}

#[test]
fn an_exhausted_hop_budget_stops_the_chain() {
    let mut asked = input("ada", "payments", vec![agent_mention("linus", 0)]);
    asked.hop = 2;
    assert_eq!(refused(OPEN, &asked), NoReferralReason::HopLimitReached);
}

#[test]
fn an_inactive_author_refers_nothing() {
    assert_eq!(
        refused(
            OPEN,
            &input("nobody", "payments", vec![agent_mention("linus", 0)])
        ),
        NoReferralReason::SourceInactive,
    );
}

#[test]
fn a_mention_of_a_deskmate_stays_in_this_conversation() {
    let referral = accepted(
        OPEN,
        &input("ada", "payments", vec![agent_mention("grace", 0)]),
    );
    assert!(!referral.crosses());
    assert_eq!(referral.to, conversation("payments", None));
    // Nothing crossed, so there is nothing to carry back.
    assert_eq!(referral.origin, None);
    assert_eq!(referral.kind, ReferralKind::Forward);
    assert_eq!(referral.child_hop, 1);
}

#[test]
fn a_same_desk_referral_keeps_its_thread_root() {
    let mut asked = input("ada", "payments", vec![agent_mention("grace", 0)]);
    asked.conversation = conversation("payments", Some(12));
    assert_eq!(
        accepted(OPEN, &asked).to,
        conversation("payments", Some(12))
    );
}

#[test]
fn a_mention_of_another_desks_member_relocates_to_their_desk() {
    let referral = accepted(
        OPEN,
        &input("ada", "payments", vec![agent_mention("linus", 0)]),
    );
    assert!(referral.crosses());
    assert_eq!(referral.to, conversation("platform", None));
    assert_eq!(referral.target_id, "linus");
    assert_eq!(
        referral.origin,
        Some(ReferralOrigin {
            conversation: conversation("payments", None),
            asker_id: "ada".to_owned(),
        }),
    );
}

#[test]
fn a_crossing_referral_lands_on_the_desk_channel_not_a_thread() {
    let mut asked = input("ada", "payments", vec![agent_mention("linus", 0)]);
    asked.conversation = conversation("payments", Some(12));
    let referral = accepted(OPEN, &asked);
    assert_eq!(referral.to.thread_root, None);
    // The back edge still names the thread that asked.
    assert_eq!(
        referral
            .origin
            .map(|origin| origin.conversation.thread_root),
        Some(Some(12)),
    );
}

#[test]
fn a_desk_mention_selects_exactly_one_responder() {
    let referral = accepted(
        OPEN,
        &input("ada", "payments", vec![desk_mention("platform", 0)]),
    );
    assert_eq!(referral.target_id, "linus");
    assert_eq!(referral.to, conversation("platform", None));
}

#[test]
fn a_desk_mention_is_inert_without_the_knob() {
    let policy = ReferralPolicy {
        reach: ReferralReach::Channels,
        ..OPEN
    };
    assert_eq!(
        refused(
            policy,
            &input("ada", "payments", vec![desk_mention("platform", 0)])
        ),
        NoReferralReason::NoReferralTarget,
    );
}

#[test]
fn a_desk_mention_naming_this_desk_refers_nothing() {
    assert_eq!(
        refused(
            OPEN,
            &input("ada", "payments", vec![desk_mention("payments", 0)])
        ),
        NoReferralReason::SelfDesk,
    );
}

#[test]
fn a_desk_mention_naming_no_desk_refers_nothing() {
    assert_eq!(
        refused(
            OPEN,
            &input("ada", "payments", vec![desk_mention("legal", 0)])
        ),
        NoReferralReason::UnknownDesk,
    );
}

#[test]
fn a_desk_whose_only_member_is_the_author_refers_nothing() {
    let members = [member("ada")];
    let roster = Roster::new(&members, &[], &[]);
    let desk_records = [desk("payments", &["grace"]), desk("platform", &["ada"])];
    let desks = DeskSet::new(&desk_records, &[], &[], &[], &[]);
    let asked = input("ada", "payments", vec![desk_mention("platform", 0)]);
    let decision = referral(OPEN, &asked, &roster, &desks).expect("well formed");
    assert_eq!(
        decision,
        ReferralDecision::None {
            reason: NoReferralReason::EmptyDesk
        },
    );
}

#[test]
fn the_lowest_offset_candidate_wins_and_a_later_one_is_never_a_fallback() {
    let asked = input(
        "ada",
        "payments",
        vec![agent_mention("ada", 3), agent_mention("linus", 40)],
    );
    assert_eq!(refused(OPEN, &asked), NoReferralReason::SelfMention);
}

#[test]
fn a_person_mention_is_skipped_rather_than_stopping_the_scan() {
    let asked = input(
        "ada",
        "payments",
        vec![
            Mention {
                target: MentionTarget::Person {
                    id: "sam".to_owned(),
                },
                text: "@sam".to_owned(),
                offset: 0,
                quiet: false,
            },
            agent_mention("linus", 10),
        ],
    );
    assert_eq!(accepted(OPEN, &asked).target_id, "linus");
}

#[test]
fn a_quiet_mention_never_refers() {
    let mut quiet = agent_mention("linus", 0);
    quiet.quiet = true;
    assert_eq!(
        refused(OPEN, &input("ada", "payments", vec![quiet])),
        NoReferralReason::NoReferralTarget,
    );
}

#[test]
fn an_inactive_target_refers_nothing() {
    assert_eq!(
        refused(
            OPEN,
            &input("ada", "payments", vec![agent_mention("nobody", 0)])
        ),
        NoReferralReason::TargetInactive,
    );
}

#[test]
fn a_target_on_no_desk_has_nowhere_to_run() {
    let members = members();
    let roster = Roster::new(&members, &[], &[]);
    let desk_records = [desk("payments", &["ada", "grace"])];
    let desks = DeskSet::new(&desk_records, &[], &[], &[], &[]);
    let asked = input("ada", "payments", vec![agent_mention("linus", 0)]);
    assert_eq!(
        referral(OPEN, &asked, &roster, &desks).expect("well formed"),
        ReferralDecision::None {
            reason: NoReferralReason::TargetDeskless
        },
    );
}

#[test]
fn everyone_is_present_in_general() {
    let asked = input("ada", "General", vec![agent_mention("linus", 0)]);
    let referral = accepted(OPEN, &asked);
    assert!(!referral.crosses());
    assert_eq!(referral.to.desk_id, "General");
}

#[test]
fn a_reply_under_a_crossing_referral_carries_one_answer_back() {
    let mut answered = input("linus", "platform", Vec::new());
    answered.conversation = conversation("platform", None);
    answered.hop = 1;
    answered.origin = Some(ReferralOrigin {
        conversation: conversation("payments", Some(12)),
        asker_id: "ada".to_owned(),
    });
    let referral = accepted(OPEN, &answered);
    assert_eq!(referral.kind, ReferralKind::Return);
    assert_eq!(referral.target_id, "ada");
    assert_eq!(referral.to, conversation("payments", Some(12)));
    assert_eq!(referral.child_hop, 2);
    // A return carries no origin, so the round trip cannot ring.
    assert_eq!(referral.origin, None);
}

#[test]
fn a_forward_of_its_own_takes_precedence_over_the_answer() {
    let mut answered = input("linus", "platform", vec![agent_mention("hedy", 0)]);
    answered.hop = 1;
    answered.origin = Some(ReferralOrigin {
        conversation: conversation("payments", None),
        asker_id: "ada".to_owned(),
    });
    let referral = accepted(OPEN, &answered);
    assert_eq!(referral.kind, ReferralKind::Forward);
    assert_eq!(referral.target_id, "hedy");
}

#[test]
fn a_return_needs_the_knob() {
    let policy = ReferralPolicy {
        returns: false,
        ..OPEN
    };
    let mut answered = input("linus", "platform", Vec::new());
    answered.hop = 1;
    answered.origin = Some(ReferralOrigin {
        conversation: conversation("payments", None),
        asker_id: "ada".to_owned(),
    });
    assert_eq!(
        refused(policy, &answered),
        NoReferralReason::NoReferralTarget
    );
}

#[test]
fn an_answer_committed_where_it_was_asked_carries_nothing_back() {
    let mut answered = input("grace", "payments", Vec::new());
    answered.hop = 1;
    answered.origin = Some(ReferralOrigin {
        conversation: conversation("payments", None),
        asker_id: "ada".to_owned(),
    });
    assert_eq!(refused(OPEN, &answered), NoReferralReason::NoReferralTarget);
}

#[test]
fn an_answer_never_returns_to_its_own_author() {
    let mut answered = input("ada", "platform", Vec::new());
    answered.hop = 1;
    answered.origin = Some(ReferralOrigin {
        conversation: conversation("payments", None),
        asker_id: "ada".to_owned(),
    });
    assert_eq!(refused(OPEN, &answered), NoReferralReason::SelfMention);
}

#[test]
fn an_answer_to_a_departed_asker_is_dropped() {
    let mut answered = input("linus", "platform", Vec::new());
    answered.hop = 1;
    answered.origin = Some(ReferralOrigin {
        conversation: conversation("payments", None),
        asker_id: "departed".to_owned(),
    });
    assert_eq!(refused(OPEN, &answered), NoReferralReason::TargetInactive);
}

#[test]
fn a_malformed_roster_is_a_typed_error() {
    let members = [member(""), member("ada")];
    let roster = Roster::new(&members, &[], &[]);
    let desk_records = desks();
    let desks = DeskSet::new(&desk_records, &[], &[], &[], &[]);
    let asked = input("ada", "payments", vec![agent_mention("linus", 0)]);
    assert!(referral(OPEN, &asked, &roster, &desks).is_err());
}

/// The compatibility statement from the spec, asserted rather than claimed.
#[test]
fn without_the_new_knobs_referral_decides_what_mention_dispatch_decides() {
    let members = members();
    let roster = Roster::new(&members, &[], &[]);
    let desk_records = desks();
    let desks = DeskSet::new(&desk_records, &[], &[], &[], &[]);
    let plain = ReferralPolicy {
        enabled: true,
        max_hops: 2,
        ..ReferralPolicy::DEFAULT
    };

    for mentions in [
        vec![agent_mention("grace", 0)],
        // The interesting case: a target who is not on this desk still gets
        // pulled into this conversation, exactly as today.
        vec![agent_mention("linus", 0)],
        vec![agent_mention("ada", 0)],
        vec![agent_mention("nobody", 0)],
        vec![desk_mention("platform", 0)],
        Vec::new(),
    ] {
        let asked = input("ada", "payments", mentions.clone());
        let dispatched = mention_dispatch(
            MentionDispatchPolicy {
                enabled: true,
                max_hops: 2,
            },
            &MentionDispatchInput {
                key: asked.key,
                conversation: asked.conversation.clone(),
                author_id: asked.author_id.clone(),
                content: asked.content.clone(),
                mentions,
                hop: asked.hop,
            },
            &roster,
        )
        .expect("well formed");
        let referred = referral(plain, &asked, &roster, &desks).expect("well formed");
        match (dispatched, referred) {
            (MentionDispatchDecision::One { request }, ReferralDecision::One { referral }) => {
                assert_eq!(request.target_id, referral.target_id);
                assert_eq!(request.source_id, referral.source_id);
                assert_eq!(request.content, referral.content);
                assert_eq!(request.conversation, referral.to);
                assert_eq!(request.child_hop, referral.child_hop);
                assert!(!referral.crosses());
            }
            (MentionDispatchDecision::None { .. }, ReferralDecision::None { .. }) => {}
            (dispatched, referred) => {
                panic!("dispatch said {dispatched:?} and referral said {referred:?}")
            }
        }
    }
}

#[test]
fn a_referral_pins_its_wire_form() {
    let referral = accepted(
        OPEN,
        &input("ada", "payments", vec![agent_mention("linus", 0)]),
    );
    let json = serde_json::to_value(&referral).expect("serializes");
    assert_eq!(
        json,
        serde_json::json!({
            "key": { "trigger_sequence": 7 },
            "kind": "forward",
            "source_id": "ada",
            "target_id": "linus",
            "content": "body",
            "from": { "desk_id": "payments", "thread_root": null },
            "to": { "desk_id": "platform", "thread_root": null },
            "origin": {
                "conversation": { "desk_id": "payments", "thread_root": null },
                "asker_id": "ada",
            },
            "child_hop": 1,
        }),
    );
    let round_tripped: Referral = serde_json::from_value(json).expect("deserializes");
    assert_eq!(round_tripped, referral);
}

#[test]
fn a_policy_and_a_refusal_pin_their_wire_forms() {
    assert_eq!(
        serde_json::to_value(ReferralPolicy::DEFAULT).expect("serializes"),
        serde_json::json!({
            "enabled": false,
            "max_hops": 0,
            "reach": "local",
            "returns": false,
        }),
    );
    assert_eq!(
        serde_json::to_value(ReferralDecision::None {
            reason: NoReferralReason::HopLimitReached
        })
        .expect("serializes"),
        serde_json::json!({ "kind": "none", "reason": "hop_limit_reached" }),
    );
    assert_eq!(ReferralPolicy::default(), ReferralPolicy::DEFAULT);
}
