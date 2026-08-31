//! Integration tests for the public crate surface.
//!
//! These tests link against the crate as a downstream consumer would: they can
//! only use what `src/lib.rs` exposes. Treat them as the regression suite for
//! the crate's public contract — if a change breaks a test here, it is a
//! breaking change for users.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use tinyteams_core::chat::{GENERAL_DESK, MAIN_THREAD_ID, is_general_chat, same_conversation};
use tinyteams_core::{
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
        mentions: vec![tinyteams_core::mention::Mention {
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
