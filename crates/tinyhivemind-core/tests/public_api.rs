//! Integration tests for the public crate surface.
//!
//! These tests link against the crate as a downstream consumer would: they can
//! only use what `src/lib.rs` exposes. Treat them as the regression suite for
//! the crate's public contract — if a change breaks a test here, it is a
//! breaking change for users.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tinyhivemind_core::chat::{GENERAL_DESK, MAIN_THREAD_ID, is_general_chat, same_conversation};
use tinyhivemind_core::{
    desk::{Desk, DeskMember, DeskOrder, DeskSet, ResponderMode},
    dispatch::{
        DispatchConversation, DispatchKey, MentionDispatchDecision, MentionDispatchInput,
        MentionDispatchPolicy, mention_dispatch,
    },
    error::Error,
    mention::{MentionAuthor, MentionTarget, direct_responder, mentioned_members, resolve},
    roster::{Person, Roster, RosterMember},
};

#[test]
fn conversation_identity_is_available_to_consumers() {
    assert!(is_general_chat(Some(MAIN_THREAD_ID)));
    assert!(is_general_chat(Some(GENERAL_DESK)));
    assert!(!is_general_chat(Some("engineering")));
}

#[test]
fn conversation_folding_is_available_to_consumers() {
    assert!(same_conversation(None, Some(GENERAL_DESK)));
    assert!(!same_conversation(Some("engineering"), Some(GENERAL_DESK)));
}

/// The constants are public because a host has to journal under them and pin
/// its own glossary against them.
#[test]
fn the_general_spellings_are_named_constants() {
    assert_eq!(MAIN_THREAD_ID, "main");
    assert_eq!(GENERAL_DESK, "General");
}

#[test]
fn desk_overlays_are_available_to_consumers_under_the_desk_module() {
    let declared = [Desk {
        id: "engineering".into(),
        name: "Engineering".into(),
        description: None,
        members: vec!["alice".into(), "bob".into()],
        responder_mode: ResponderMode::Lead,
    }];
    let additions = [DeskMember {
        desk_id: "engineering".into(),
        agent_id: "cara".into(),
    }];
    let orders = [DeskOrder {
        desk_id: "engineering".into(),
        ordered: vec!["cara".into(), "alice".into(), "bob".into()],
    }];
    let desks = DeskSet::new(&declared, &[], &additions, &orders, &[]);

    assert_eq!(desks.resolve_id("Engineering").unwrap(), "engineering");
    assert_eq!(
        desks.members("engineering").unwrap(),
        ["cara", "alice", "bob"]
    );
    assert_eq!(desks.lead("Engineering").unwrap(), Some("cara"));
}

#[test]
fn desk_failures_are_available_as_the_crate_error_type() {
    let desks = DeskSet::new(&[], &[], &[], &[], &[]);
    assert_eq!(
        desks.resolve_id("unknown").unwrap_err(),
        Error::UnknownDesk {
            identity: "unknown".into()
        }
    );
}

#[test]
fn roster_and_mention_decisions_are_available_to_consumers() {
    let members = [
        RosterMember {
            id: "alice".into(),
            name: Some("Alice".into()),
        },
        RosterMember {
            id: "bob".into(),
            name: Some("Bob".into()),
        },
    ];
    let people = [Person {
        id: "person-1".into(),
        label: "Operator".into(),
    }];
    let declared = [Desk {
        id: "engineering".into(),
        name: "Engineering".into(),
        description: None,
        members: vec!["alice".into(), "bob".into()],
        responder_mode: ResponderMode::Lead,
    }];
    let roster = Roster::new(&members, &people, &[]);
    let desks = DeskSet::new(&declared, &[], &[], &[], &[]);

    let mentions = resolve(
        "@Alice please coordinate with @#Engineering",
        None,
        &MentionAuthor::Person {
            id: "person-1".into(),
        },
        &roster,
        &desks,
    );

    assert_eq!(direct_responder(&mentions, &roster), Some("alice"));
    assert_eq!(
        mentioned_members(
            &mentions,
            Some("engineering"),
            Some("alice"),
            &roster,
            &desks
        ),
        ["bob"]
    );
    assert_eq!(
        mentions[0].target,
        MentionTarget::Agent { id: "alice".into() }
    );
}

#[test]
fn bounded_dispatch_decision_is_available_to_consumers() {
    let members = [
        RosterMember {
            id: "alice".into(),
            name: None,
        },
        RosterMember {
            id: "bob".into(),
            name: None,
        },
    ];
    let roster = Roster::new(&members, &[], &[]);
    let input = MentionDispatchInput {
        key: DispatchKey {
            trigger_sequence: 8,
        },
        conversation: DispatchConversation {
            desk_id: "engineering".into(),
            thread_root: None,
        },
        author_id: "alice".into(),
        content: "@bob".into(),
        mentions: vec![tinyhivemind_core::mention::Mention {
            target: MentionTarget::Agent { id: "bob".into() },
            text: "@bob".into(),
            offset: 0,
            quiet: false,
        }],
        hop: 0,
    };
    let decision = mention_dispatch(
        MentionDispatchPolicy {
            enabled: true,
            max_hops: 2,
        },
        &input,
        &roster,
    )
    .unwrap();
    assert!(
        matches!(decision, MentionDispatchDecision::One { request } if request.target_id == "bob")
    );
}

#[test]
fn selection_ranking_is_available_to_consumers() {
    use tinyhivemind_core::select::{
        Candidate, MatchField, MatchKind, Pattern, SELECT_LIMIT, rank, rank_pattern, regex_source,
        score, score_pattern,
    };

    let candidates = [
        Candidate::new("alice", "Alice Nakamura"),
        Candidate::new("bob", "Bob Ferrante").with_detail("reviews Alice's changes"),
    ];
    let hits = rank("alice", &candidates, SELECT_LIMIT);
    // The id is the query exactly; the label only starts with it, so the id is
    // the field reported.
    assert_eq!(hits[0].id, "alice");
    assert_eq!(hits[0].field, MatchField::Id);
    assert_eq!(hits[0].kind, MatchKind::Exact);
    assert_eq!(hits[1].id, "bob");
    assert_eq!(hits[1].field, MatchField::Detail);
    assert_eq!(
        rank("nakamura", &candidates, SELECT_LIMIT)[0].field,
        MatchField::Label
    );

    assert_eq!(
        rank_pattern(&Pattern::Text("alice"), &candidates, SELECT_LIMIT).len(),
        hits.len()
    );
    assert_eq!(
        score("alice", "Alice"),
        score_pattern(&Pattern::Text("alice"), "Alice")
    );
    assert_eq!(regex_source("/^ali/"), Some("^ali"));
    assert_eq!(SELECT_LIMIT, 8);
}

#[test]
fn name_searches_are_available_to_consumers() {
    use tinyhivemind_core::{find, select::SELECT_LIMIT};

    let members = [
        RosterMember {
            id: "alice".into(),
            name: Some("Alice Nakamura".into()),
        },
        RosterMember {
            id: "bob".into(),
            name: Some("Bob Ferrante".into()),
        },
    ];
    let people = [Person {
        id: "u-1".into(),
        label: "Dana Okoro".into(),
    }];
    let roster = Roster::new(&members, &people, &[]);
    let declared = [Desk {
        id: "engineering".into(),
        name: "Engineering".into(),
        description: Some("Ship the product".into()),
        members: vec!["alice".into()],
        responder_mode: ResponderMode::Lead,
    }];
    let desks = DeskSet::new(&declared, &[], &[], &[], &[]);

    assert_eq!(find::agents("naka", &roster, SELECT_LIMIT)[0].id, "alice");
    assert_eq!(find::people("okoro", &roster, SELECT_LIMIT)[0].id, "u-1");
    assert_eq!(
        find::desks("product", &desks, SELECT_LIMIT)[0].id,
        "engineering"
    );
}

/// Two agents, one on each of two desks.
fn two_desks() -> ([RosterMember; 2], [Desk; 2]) {
    (
        [
            RosterMember {
                id: "ada".into(),
                name: Some("Ada".into()),
            },
            RosterMember {
                id: "linus".into(),
                name: Some("Linus".into()),
            },
        ],
        [
            Desk {
                id: "payments".into(),
                name: "Payments".into(),
                description: None,
                members: vec!["ada".into()],
                responder_mode: ResponderMode::Lead,
            },
            Desk {
                id: "platform".into(),
                name: "Platform".into(),
                description: None,
                members: vec!["linus".into()],
                responder_mode: ResponderMode::Lead,
            },
        ],
    )
}

#[test]
fn a_referral_crosses_a_desk_and_finds_its_way_home() {
    use tinyhivemind_core::referral::{
        ReferralDecision, ReferralInput, ReferralKind, ReferralOrigin, ReferralPolicy,
        ReferralReach, referral,
    };

    let (members, records) = two_desks();
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&records, &[], &[], &[], &[]);
    let policy = ReferralPolicy {
        enabled: true,
        max_hops: 2,
        reach: ReferralReach::Desks,
        returns: true,
    };

    // Ada asks the platform desk, from inside a thread on her own desk.
    let asking = ReferralInput {
        key: DispatchKey {
            trigger_sequence: 12,
        },
        conversation: DispatchConversation {
            desk_id: "payments".into(),
            thread_root: Some(9),
        },
        author_id: "ada".into(),
        content: "@#platform does the gateway retry on 503?".into(),
        mentions: resolve(
            "@#platform does the gateway retry on 503?",
            None,
            &MentionAuthor::Agent { id: "ada".into() },
            &roster,
            &desks,
        ),
        hop: 0,
        origin: None,
    };
    let ReferralDecision::One { referral: out } =
        referral(policy, &asking, &roster, &desks).expect("well formed")
    else {
        panic!("the desk mention should have crossed");
    };
    assert_eq!(out.kind, ReferralKind::Forward);
    assert_eq!(out.target_id, "linus");
    assert_eq!(out.to.desk_id, "platform");
    // A crossing referral lands on the desk channel, never in a thread.
    assert_eq!(out.to.thread_root, None);
    assert_eq!(out.child_hop, 1);

    // Linus answers on his own desk, and the answer finds the thread that asked.
    let answering = ReferralInput {
        key: DispatchKey {
            trigger_sequence: 4,
        },
        conversation: DispatchConversation {
            desk_id: "platform".into(),
            thread_root: None,
        },
        author_id: "linus".into(),
        content: "It retries twice with jitter.".into(),
        mentions: Vec::new(),
        hop: out.child_hop,
        origin: out.origin.clone(),
    };
    let ReferralDecision::One { referral: home } =
        referral(policy, &answering, &roster, &desks).expect("well formed")
    else {
        panic!("the answer should have come home");
    };
    assert_eq!(home.kind, ReferralKind::Return);
    assert_eq!(home.target_id, "ada");
    assert_eq!(
        home.to,
        DispatchConversation {
            desk_id: "payments".into(),
            thread_root: Some(9),
        },
    );
    assert_eq!(home.child_hop, 2);
    assert_eq!(home.origin, None);
    assert_eq!(
        out.origin,
        Some(ReferralOrigin {
            conversation: DispatchConversation {
                desk_id: "payments".into(),
                thread_root: Some(9),
            },
            asker_id: "ada".into(),
        }),
    );

    // And the round trip is over: Ada's relay may not start another.
    let relaying = ReferralInput {
        hop: home.child_hop,
        origin: None,
        conversation: home.to.clone(),
        author_id: "ada".into(),
        ..asking
    };
    assert!(matches!(
        referral(policy, &relaying, &roster, &desks).expect("well formed"),
        ReferralDecision::None { .. },
    ));
}
