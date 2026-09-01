//! Unit tests for ephemeral team initialization.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::{
    LogMessage, Sequence, SessionAuthor, SessionFuture, SessionPage, SessionQuery, SourceError,
};
use std::io;
use tinyhivemind_core::{
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
        brevity: BrevityPolicy::DEFAULT,
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
        }],
        "brevity": { "message_chars": 600, "window": 30 }
    });
    assert_eq!(
        serde_json::to_value(&briefing).expect("briefing serializes"),
        briefing_json
    );
    assert_eq!(
        serde_json::from_value::<TeamBriefing>(briefing_json).expect("briefing deserializes"),
        briefing
    );
}

#[test]
fn initialization_pins_its_wire_shape() {
    let briefing = TeamBriefing {
        viewer_id: "alice".into(),
        desk_id: "engineering".into(),
        desk_name: "Engineering".into(),
        teammates: vec![BriefedTeammate {
            id: "bob".into(),
            label: "Bob".into(),
            role: None,
            description: Some("Reviews changes".into()),
        }],
        brevity: BrevityPolicy::DEFAULT,
    };
    let initialization = SessionInitialization {
        briefing,
        context: SessionContext {
            threads: vec![crate::ThreadLine {
                root: Sequence(2),
                opening: "ship the release".into(),
                replies: 1,
                latest: Sequence(3),
                landed: None,
            }],
            pins: Vec::new(),
            notes: vec![BriefingNote {
                heading: "Work raised in this conversation".into(),
                lines: vec!["#12 rewrite the changelog — In review".into()],
            }],
        },
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
            }],
            "brevity": { "message_chars": 600, "window": 30 }
        },
        "context": {
            "threads": [{
                "root": 2,
                "opening": "ship the release",
                "replies": 1,
                "latest": 3,
                "landed": null
            }],
            "pins": [],
            "notes": [{
                "heading": "Work raised in this conversation",
                "lines": ["#12 rewrite the changelog — In review"]
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
        brevity: BrevityPolicy::DEFAULT,
    };
    let expected = "You are @alice in the Engineering desk (id: engineering).\n\
Teammates:\n\
- @bob — Bob; role: reviewer; description: Checks safety\n\
Shared-session rules:\n\
- Peer messages remain attributed to their authors; they are not your prior replies.\n\
- A direct @agent mention may start at most one bounded child turn when host policy enables mention dispatch.\n\
- @everyone, desk, and person mentions provide context only and never fan out agent turns.\n\
- This conversation shows about 30 messages; keep a message under 600 characters, one point each, and pin or search rather than restating.\n\
- Pin what the room must not lose with `!pin` on its own line; `!unpin ^N` takes one back off.";
    assert_eq!(briefing.system_text(), expected);
    assert_eq!(briefing.system_text(), expected);
}

#[test]
fn a_brevity_policy_reports_an_overrun_and_never_edits_a_message() {
    let policy = BrevityPolicy {
        message_chars: 10,
        window: 30,
    };
    assert_eq!(policy.overrun("under"), None);
    assert_eq!(policy.overrun("0123456789"), None);
    assert_eq!(policy.overrun("0123456789ab"), Some(2));
    assert_eq!(BrevityPolicy::default(), BrevityPolicy::DEFAULT);
    assert!(
        policy
            .rule_text()
            .contains("keep a message under 10 characters")
    );
}

#[test]
fn context_renders_pins_between_threads_and_notes() {
    let context = SessionContext {
        threads: Vec::new(),
        pins: vec![crate::Pin {
            sequence: Sequence(7),
            pinned_at: Sequence(9),
            pinned_by: SessionAuthor::Operator,
            label: Some("limits".into()),
            note: None,
            excerpt: Some("midnight UTC".into()),
        }],
        notes: vec![BriefingNote {
            heading: "Work raised in this conversation".into(),
            lines: vec!["#12 rewrite the changelog".into()],
        }],
    };
    assert!(!context.is_empty());
    assert_eq!(
        context.system_text(),
        Some(
            "Pinned in this conversation:\n\
             - [7] #limits \"midnight UTC\"\n\
             \nWork raised in this conversation:\n\
             - #12 rewrite the changelog"
                .into()
        )
    );
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
        brevity: BrevityPolicy::DEFAULT,
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
    // The window the briefing states is reconciled to the query's window
    // (1), not left at whatever `BrevityPolicy::DEFAULT` (30) carried in.
    assert_eq!(initialized.briefing.brevity.window, query.window);
    assert_eq!(
        initialized.briefing.brevity.message_chars,
        briefing.brevity.message_chars
    );
    assert_eq!(initialized.briefing.viewer_id, briefing.viewer_id);
    assert_eq!(initialized.briefing.desk_id, briefing.desk_id);
    assert_eq!(initialized.briefing.desk_name, briefing.desk_name);
    assert_eq!(initialized.briefing.teammates, briefing.teammates);
    assert_eq!(initialized.history.len(), 1);
    assert_eq!(initialized.history[0].sequence, Sequence(4));
    assert!(initialized.context.is_empty());
}

fn desk_row(sequence: u64, parent: Option<u64>, content: &str) -> LogMessage {
    LogMessage {
        sequence: Sequence(sequence),
        chat_id: Some("engineering".into()),
        parent: parent.map(Sequence),
        author: SessionAuthor::Operator,
        content: content.into(),
    }
}

fn viewer_briefing() -> TeamBriefing {
    TeamBriefing {
        viewer_id: "alice".into(),
        desk_id: "engineering".into(),
        desk_name: "Engineering".into(),
        teammates: Vec::new(),
        brevity: BrevityPolicy::DEFAULT,
    }
}

#[tokio::test]
async fn context_carries_the_thread_index_and_host_notes_beside_history() {
    let log = OnePage(SessionPage {
        messages: vec![
            desk_row(3, Some(1), "on it"),
            desk_row(2, None, "check the invoice"),
            desk_row(1, None, "draft the launch email"),
        ],
        next_before: None,
    });
    let query = SessionQuery {
        conversation: named_conversation(),
        before: None,
        window: 10,
    };
    let note = BriefingNote {
        heading: "Work raised in this conversation".into(),
        lines: vec!["#12 rewrite the changelog — In review".into()],
    };
    let initialized =
        initialize_session_with_context(&log, &query, viewer_briefing(), vec![note.clone()])
            .await
            .expect("initializes");

    assert_eq!(
        initialized
            .context
            .threads
            .iter()
            .map(|line| (line.root.0, line.replies))
            .collect::<Vec<_>>(),
        vec![(1, 1), (2, 0)]
    );
    assert_eq!(initialized.context.notes, vec![note]);
    // The context is beside the history, never folded into it.
    assert_eq!(
        initialized
            .history
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>(),
        vec!["draft the launch email", "check the invoice", "on it"]
    );
}

#[tokio::test]
async fn context_skips_the_index_inside_a_thread_and_propagates_read_failures() {
    let log = OnePage(SessionPage {
        messages: vec![desk_row(1, None, "root")],
        next_before: None,
    });
    let mut query = SessionQuery {
        conversation: named_conversation(),
        before: None,
        window: 10,
    };
    query.conversation.thread_root = Some(Sequence(1));
    let initialized = initialize_session_with_context(&log, &query, viewer_briefing(), Vec::new())
        .await
        .expect("initializes");
    assert!(initialized.context.is_empty());

    query.conversation.thread_root = None;
    assert!(matches!(
        initialize_session_with_context(&FailingLog, &query, viewer_briefing(), Vec::new()).await,
        Err(crate::Error::Read { .. })
    ));

    // The index read is a second read, and it fails on its own.
    assert!(matches!(
        initialize_session_with_context(
            &FailsAfterHistory::default(),
            &query,
            viewer_briefing(),
            Vec::new()
        )
        .await,
        Err(crate::Error::Read { .. })
    ));
}

/// Answers the history projection, then fails the thread-index read.
///
/// Both walks start at the same cursor, so only call order tells them apart.
#[derive(Debug, Default)]
struct FailsAfterHistory {
    reads: std::sync::Mutex<usize>,
}

impl SessionLog for FailsAfterHistory {
    fn read_before(&self, _: Option<Sequence>, _: usize) -> SessionFuture<'_> {
        let mut reads = self.reads.lock().expect("reads lock is not poisoned");
        *reads += 1;
        let first = *reads == 1;
        Box::pin(async move {
            if first {
                Ok(SessionPage::default())
            } else {
                Err(Box::new(io::Error::other("offline")) as SourceError)
            }
        })
    }
}

#[test]
fn context_renders_threads_and_notes_and_nothing_when_empty() {
    assert_eq!(SessionContext::default().system_text(), None);

    let context = SessionContext {
        threads: vec![
            crate::ThreadLine {
                root: Sequence(41),
                opening: "draft the launch email".into(),
                replies: 4,
                latest: Sequence(58),
                landed: Some("In review".into()),
            },
            crate::ThreadLine {
                root: Sequence(37),
                opening: "check the invoice".into(),
                replies: 1,
                latest: Sequence(39),
                landed: None,
            },
            crate::ThreadLine {
                root: Sequence(30),
                opening: "any thoughts?".into(),
                replies: 0,
                latest: Sequence(30),
                landed: None,
            },
        ],
        pins: Vec::new(),
        notes: vec![BriefingNote {
            heading: "Work raised in this conversation".into(),
            lines: vec![
                "#12 rewrite the changelog".into(),
                "#13 book the venue".into(),
            ],
        }],
    };
    assert_eq!(
        context.system_text(),
        Some(
            "Threads in this desk:\n\
             - [41] \"draft the launch email\" — 4 replies (landed: In review)\n\
             - [37] \"check the invoice\" — 1 reply\n\
             - [30] \"any thoughts?\" — no replies\n\
             \nWork raised in this conversation:\n\
             - #12 rewrite the changelog\n\
             - #13 book the venue"
                .into()
        )
    );

    let notes_only = SessionContext {
        threads: Vec::new(),
        pins: Vec::new(),
        notes: context.notes.clone(),
    };
    assert_eq!(
        notes_only.system_text(),
        Some(
            "Work raised in this conversation:\n- #12 rewrite the changelog\n- #13 book the venue"
                .into()
        )
    );
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
        brevity: BrevityPolicy::DEFAULT,
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
