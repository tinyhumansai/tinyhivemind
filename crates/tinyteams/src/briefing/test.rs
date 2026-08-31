//! Unit tests for ephemeral team initialization.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::{
    LogMessage, Sequence, SessionAuthor, SessionFuture, SessionPage, SessionQuery, SourceError,
};
use std::io;
use tinyteams_core::{
    desk::{Desk, DeskMember, DeskOrder, ResponderMode},
    roster::{Person, RosterMember},
};

fn named_conversation() -> Conversation {
    Conversation {
        desk_id: "engineering".into(),
        desk_name: "Engineering".into(),
        thread_root: None,
    }
}

fn members() -> Vec<RosterMember> {
    vec![
        RosterMember {
            id: "alice".into(),
            name: Some("Alice".into()),
        },
        RosterMember {
            id: "bob".into(),
            name: Some("Bob".into()),
        },
        RosterMember {
            id: "retired".into(),
            name: None,
        },
        RosterMember {
            id: "carol".into(),
            name: None,
        },
    ]
}

fn desk_records() -> Vec<Desk> {
    vec![Desk {
        id: "engineering".into(),
        name: "Engineering".into(),
        description: Some("Build".into()),
        members: vec!["bob".into(), "unknown".into(), "alice".into()],
        responder_mode: ResponderMode::Lead,
    }]
}

#[test]
fn briefing_records_pin_their_wire_shape() {
    let teammate = BriefedTeammate {
        id: "bob".into(),
        label: "Bob".into(),
        role: None,
        description: Some("Reviews changes".into()),
    };
    assert_eq!(
        serde_json::to_value(&teammate).expect("teammate serializes"),
        serde_json::json!({
            "id": "bob",
            "label": "Bob",
            "role": null,
            "description": "Reviews changes"
        })
    );
    assert_eq!(
        serde_json::from_value::<BriefedTeammate>(serde_json::json!({
            "id": "bob",
            "label": "Bob",
            "role": null,
            "description": "Reviews changes"
        }))
        .expect("teammate deserializes"),
        teammate
    );

    let briefing = TeamBriefing {
        viewer_id: "alice".into(),
        desk_id: "engineering".into(),
        desk_name: "Engineering".into(),
        teammates: vec![teammate],
    };
    let briefing_json = serde_json::json!({
        "viewer_id": "alice",
        "desk_id": "engineering",
        "desk_name": "Engineering",
        "teammates": [{
            "id": "bob",
            "label": "Bob",
            "role": null,
            "description": "Reviews changes"
        }]
    });
    assert_eq!(
        serde_json::to_value(&briefing).expect("briefing serializes"),
        briefing_json
    );
    assert_eq!(
        serde_json::from_value::<TeamBriefing>(briefing_json).expect("briefing deserializes"),
        briefing
    );

    let initialization = SessionInitialization {
        briefing,
        history: vec![crate::SessionMessage {
            sequence: Sequence(4),
            author: SessionAuthor::Operator,
            content: "hello".into(),
        }],
    };
    let initialization_json = serde_json::json!({
        "briefing": {
            "viewer_id": "alice",
            "desk_id": "engineering",
            "desk_name": "Engineering",
            "teammates": [{
                "id": "bob",
                "label": "Bob",
                "role": null,
                "description": "Reviews changes"
            }]
        },
        "history": [{
            "sequence": 4,
            "author": {"type":"operator"},
            "content": "hello"
        }]
    });
    assert_eq!(
        serde_json::to_value(&initialization).expect("initialization serializes"),
        initialization_json
    );
    assert_eq!(
        serde_json::from_value::<SessionInitialization>(initialization_json)
            .expect("initialization deserializes"),
        initialization
    );
}

#[test]
fn briefing_wire_records_require_every_field() {
    assert!(
        serde_json::from_value::<BriefedTeammate>(serde_json::json!({
            "id": "bob",
            "role": null,
            "description": null
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<TeamBriefing>(serde_json::json!({
            "viewer_id": "alice",
            "desk_id": "engineering",
            "desk_name": "Engineering"
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<SessionInitialization>(serde_json::json!({
            "briefing": {
                "viewer_id": "alice",
                "desk_id": "engineering",
                "desk_name": "Engineering",
                "teammates": []
            }
        }))
        .is_err()
    );
}

#[test]
fn named_desk_uses_effective_order_and_filters_viewer_retired_unknown_and_duplicates() {
    let members = members();
    let retired = vec!["retired".into()];
    let roster = Roster::new(&members, &[], &retired);
    let desks = desk_records();
    let additions = vec![
        DeskMember {
            desk_id: "engineering".into(),
            agent_id: "carol".into(),
        },
        DeskMember {
            desk_id: "engineering".into(),
            agent_id: "bob".into(),
        },
        DeskMember {
            desk_id: "engineering".into(),
            agent_id: "retired".into(),
        },
    ];
    let orders = vec![DeskOrder {
        desk_id: "engineering".into(),
        ordered: vec![
            "carol".into(),
            "unknown".into(),
            "alice".into(),
            "bob".into(),
        ],
    }];
    let desk_set = DeskSet::new(&desks, &[], &additions, &orders, &retired);
    let briefing = TeamBriefing::from_snapshots("alice", &named_conversation(), &desk_set, &roster)
        .expect("valid snapshots");
    assert_eq!(
        briefing
            .teammates
            .iter()
            .map(|teammate| (teammate.id.as_str(), teammate.label.as_str()))
            .collect::<Vec<_>>(),
        vec![("carol", "carol"), ("bob", "Bob")]
    );
    assert!(briefing.teammates.iter().all(|item| item.role.is_none()));
}

#[test]
fn general_uses_the_active_roster_in_roster_order() {
    let members = members();
    let retired = vec!["retired".into()];
    let roster = Roster::new(&members, &[], &retired);
    let desks = DeskSet::new(&[], &[], &[], &[], &retired);
    let conversation = Conversation {
        desk_id: "main".into(),
        desk_name: "General".into(),
        thread_root: None,
    };
    let briefing = TeamBriefing::from_snapshots("bob", &conversation, &desks, &roster)
        .expect("valid snapshots");
    assert_eq!(
        briefing
            .teammates
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["alice", "carol"]
    );
}

#[test]
fn invalid_snapshots_return_the_precise_core_source() {
    let members = vec![RosterMember {
        id: " ".into(),
        name: None,
    }];
    let roster = Roster::new(&members, &[], &[]);
    let desks = DeskSet::new(&[], &[], &[], &[], &[]);
    let error = TeamBriefing::from_snapshots("viewer", &named_conversation(), &desks, &roster)
        .expect_err("blank member id fails");
    assert!(matches!(error, crate::Error::Core { .. }));
    assert!(std::error::Error::source(&error).is_some());
}

#[test]
fn system_text_is_deterministic_and_states_coordination_rules() {
    let briefing = TeamBriefing {
        viewer_id: "alice".into(),
        desk_id: "engineering".into(),
        desk_name: "Engineering".into(),
        teammates: vec![BriefedTeammate {
            id: "bob".into(),
            label: "Bob".into(),
            role: Some("reviewer".into()),
            description: Some("Checks safety".into()),
        }],
    };
    let expected = "You are @alice in the Engineering desk (id: engineering).\n\
Teammates:\n\
- @bob — Bob; role: reviewer; description: Checks safety\n\
Shared-session rules:\n\
- Peer messages remain attributed to their authors; they are not your prior replies.\n\
- @everyone adds team context only and never fans out agent turns.\n\
- Mentions are context only until mention dispatch is introduced in P7.";
    assert_eq!(briefing.system_text(), expected);
    assert_eq!(briefing.system_text(), expected);
}

#[derive(Debug)]
struct OnePage(SessionPage);

impl SessionLog for OnePage {
    fn read_before(&self, _: Option<Sequence>, _: usize) -> SessionFuture<'_> {
        Box::pin(async { Ok(self.0.clone()) })
    }
}

#[tokio::test]
async fn initialization_keeps_briefing_separate_from_history() {
    let briefing = TeamBriefing {
        viewer_id: "alice".into(),
        desk_id: "engineering".into(),
        desk_name: "Engineering".into(),
        teammates: Vec::new(),
    };
    let row = LogMessage {
        sequence: Sequence(4),
        chat_id: Some("engineering".into()),
        parent: None,
        author: SessionAuthor::Operator,
        content: "hello".into(),
    };
    let query = SessionQuery {
        conversation: named_conversation(),
        before: None,
        window: 1,
    };
    let initialized = initialize_session(
        &OnePage(SessionPage {
            messages: vec![row],
            next_before: None,
        }),
        &query,
        briefing.clone(),
    )
    .await
    .expect("initializes");
    assert_eq!(initialized.briefing, briefing);
    assert_eq!(initialized.history.len(), 1);
    assert_eq!(initialized.history[0].sequence, Sequence(4));
}

#[derive(Debug)]
struct FailingLog;

impl SessionLog for FailingLog {
    fn read_before(&self, _: Option<Sequence>, _: usize) -> SessionFuture<'_> {
        Box::pin(async { Err(Box::new(io::Error::other("offline")) as SourceError) })
    }
}

#[tokio::test]
async fn initialization_propagates_projection_errors() {
    let briefing = TeamBriefing {
        viewer_id: "alice".into(),
        desk_id: "engineering".into(),
        desk_name: "Engineering".into(),
        teammates: Vec::new(),
    };
    let query = SessionQuery {
        conversation: named_conversation(),
        before: None,
        window: 1,
    };
    assert!(matches!(
        initialize_session(&FailingLog, &query, briefing).await,
        Err(crate::Error::Read { .. })
    ));
}

#[test]
fn snapshot_constructor_does_not_require_people_or_host_role_types() {
    let people = vec![Person {
        id: "person".into(),
        label: "Person".into(),
    }];
    let members = members();
    let roster = Roster::new(&members, &people, &[]);
    let desks = desk_records();
    let desk_set = DeskSet::new(&desks, &[], &[], &[], &[]);
    let briefing = TeamBriefing::from_snapshots("alice", &named_conversation(), &desk_set, &roster)
        .expect("constructs without host types");
    assert_eq!(briefing.teammates[0].id, "bob");
}
