//! Unit tests for validated attributed projection.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::Error;
use std::{
    collections::VecDeque,
    io,
    sync::{Arc, Mutex},
};

fn assert_wire_round_trip<T>(value: &T, expected: serde_json::Value)
where
    T: serde::Serialize + serde::de::DeserializeOwned + Eq + std::fmt::Debug,
{
    assert_eq!(serde_json::to_value(value).expect("serializes"), expected);
    assert_eq!(
        serde_json::from_value::<T>(expected).expect("deserializes"),
        *value
    );
}

type Calls = Arc<Mutex<Vec<(Option<Sequence>, usize)>>>;

#[derive(Debug)]
struct FakeLog {
    pages: Mutex<VecDeque<std::result::Result<SessionPage, SourceError>>>,
    calls: Calls,
}

impl FakeLog {
    fn new(pages: Vec<SessionPage>) -> Self {
        Self {
            pages: Mutex::new(pages.into_iter().map(Ok).collect()),
            calls: Arc::default(),
        }
    }

    fn failing() -> Self {
        Self {
            pages: Mutex::new(VecDeque::from([Err(
                Box::new(io::Error::other("offline")) as SourceError
            )])),
            calls: Arc::default(),
        }
    }

    fn call_count(&self) -> usize {
        self.calls.lock().expect("calls lock is not poisoned").len()
    }
}

impl SessionLog for FakeLog {
    fn read_before(&self, before: Option<Sequence>, limit: usize) -> SessionFuture<'_> {
        self.calls
            .lock()
            .expect("calls lock is not poisoned")
            .push((before, limit));
        Box::pin(async move {
            self.pages
                .lock()
                .expect("pages lock is not poisoned")
                .pop_front()
                .unwrap_or_else(|| Ok(SessionPage::default()))
        })
    }
}

fn conversation() -> Conversation {
    Conversation {
        desk_id: "engineering".into(),
        desk_name: "Engineering".into(),
        thread_root: None,
    }
}

fn query(window: usize) -> SessionQuery {
    SessionQuery {
        conversation: conversation(),
        before: None,
        window,
    }
}

fn message(sequence: u64, chat: Option<&str>, parent: Option<u64>, content: &str) -> LogMessage {
    LogMessage {
        sequence: Sequence(sequence),
        chat_id: chat.map(str::to_owned),
        parent: parent.map(Sequence),
        author: SessionAuthor::Agent {
            id: format!("agent-{sequence}"),
            label: format!("Agent {sequence}"),
        },
        content: content.into(),
    }
}

fn page(messages: Vec<LogMessage>, next: Option<u64>) -> SessionPage {
    SessionPage {
        messages,
        next_before: next.map(Sequence),
    }
}

#[test]
fn session_author_variants_pin_their_wire_shape() {
    let authors = [
        (
            SessionAuthor::Operator,
            serde_json::json!({"type":"operator"}),
        ),
        (
            SessionAuthor::Person {
                id: "p1".into(),
                label: "Pat".into(),
            },
            serde_json::json!({"type":"person","id":"p1","label":"Pat"}),
        ),
        (
            SessionAuthor::Agent {
                id: "a1".into(),
                label: "Ada".into(),
            },
            serde_json::json!({"type":"agent","id":"a1","label":"Ada"}),
        ),
        (
            SessionAuthor::System {
                kind: "workflow".into(),
                label: "Build".into(),
            },
            serde_json::json!({"type":"system","kind":"workflow","label":"Build"}),
        ),
    ];
    for (author, expected) in authors {
        assert_wire_round_trip(&author, expected);
    }
}

#[test]
fn session_records_pin_their_wire_shape() {
    assert_wire_round_trip(&Sequence(7), serde_json::json!(7));
    assert_wire_round_trip(
        &Conversation {
            desk_id: "engineering".into(),
            desk_name: "Engineering".into(),
            thread_root: Some(Sequence(4)),
        },
        serde_json::json!({
            "desk_id": "engineering",
            "desk_name": "Engineering",
            "thread_root": 4
        }),
    );

    let raw = LogMessage {
        sequence: Sequence(9),
        chat_id: Some("engineering".into()),
        parent: Some(Sequence(4)),
        author: SessionAuthor::Operator,
        content: "hello".into(),
    };
    assert_wire_round_trip(
        &raw,
        serde_json::json!({
            "sequence": 9,
            "chat_id": "engineering",
            "parent": 4,
            "author": {"type":"operator"},
            "content": "hello"
        }),
    );
    assert_wire_round_trip(
        &SessionPage {
            messages: vec![raw],
            next_before: Some(Sequence(9)),
        },
        serde_json::json!({
            "messages": [{
                "sequence": 9,
                "chat_id": "engineering",
                "parent": 4,
                "author": {"type":"operator"},
                "content": "hello"
            }],
            "next_before": 9
        }),
    );
    assert_wire_round_trip(
        &SessionMessage {
            sequence: Sequence(9),
            author: SessionAuthor::Operator,
            content: "hello".into(),
        },
        serde_json::json!({
            "sequence": 9,
            "author": {"type":"operator"},
            "content": "hello"
        }),
    );
    assert_wire_round_trip(
        &SessionQuery {
            conversation: Conversation {
                desk_id: "engineering".into(),
                desk_name: "Engineering".into(),
                thread_root: None,
            },
            before: Some(Sequence(10)),
            window: 30,
        },
        serde_json::json!({
            "conversation": {
                "desk_id": "engineering",
                "desk_name": "Engineering",
                "thread_root": null
            },
            "before": 10,
            "window": 30
        }),
    );
    assert_eq!(Sequence(7).to_string(), "7");
}

#[test]
fn dispatch_scope_canonicalizes_every_general_alias_and_preserves_exact_threads() {
    use tinyhivemind_core::{chat::GENERAL_DESK, dispatch::DispatchConversation};

    let aliases = ["", "main", "MAIN", "General", "GENERAL"];
    for alias in aliases {
        for (desk_id, desk_name) in [(alias, "Ordinary label"), ("opaque-id", alias)] {
            let conversation = Conversation {
                desk_id: desk_id.into(),
                desk_name: desk_name.into(),
                thread_root: Some(Sequence(17)),
            };
            assert_eq!(
                DispatchConversation::from(&conversation),
                DispatchConversation {
                    desk_id: GENERAL_DESK.into(),
                    thread_root: Some(17),
                }
            );
        }
    }

    let channel = DispatchConversation::from(&Conversation {
        desk_id: "main".into(),
        desk_name: "General".into(),
        thread_root: None,
    });
    let first_thread = DispatchConversation::from(&Conversation {
        desk_id: "General".into(),
        desk_name: "General".into(),
        thread_root: Some(Sequence(17)),
    });
    let second_thread = DispatchConversation::from(&Conversation {
        desk_id: "MAIN".into(),
        desk_name: "General".into(),
        thread_root: Some(Sequence(18)),
    });
    assert_ne!(channel, first_thread);
    assert_ne!(first_thread, second_thread);

    let named = DispatchConversation::from(&Conversation {
        desk_id: "Engineering".into(),
        desk_name: "engineering".into(),
        thread_root: Some(Sequence(17)),
    });
    assert_eq!(named.desk_id, "Engineering");
}

#[test]
fn session_wire_records_require_every_field() {
    assert!(
        serde_json::from_value::<Conversation>(serde_json::json!({
            "desk_id": "engineering",
            "thread_root": null
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<LogMessage>(serde_json::json!({
            "sequence": 1,
            "chat_id": null,
            "parent": null,
            "author": {"type":"operator"}
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<SessionPage>(serde_json::json!({
            "next_before": null
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<SessionMessage>(serde_json::json!({
            "sequence": 1,
            "author": {"type":"operator"}
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<SessionQuery>(serde_json::json!({
            "conversation": {
                "desk_id": "engineering",
                "desk_name": "Engineering",
                "thread_root": null
            },
            "before": null
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<SessionAuthor>(serde_json::json!({
            "type": "person",
            "id": "p1"
        }))
        .is_err()
    );
}

#[test]
fn session_log_is_object_safe() {
    fn accepts_object(_: &dyn SessionLog) {}
    accepts_object(&FakeLog::new(Vec::new()));
}

#[tokio::test]
async fn zero_window_performs_no_read() {
    let log = FakeLog::new(Vec::new());
    assert!(
        project_session(&log, &query(0))
            .await
            .expect("projects")
            .is_empty()
    );
    assert_eq!(log.call_count(), 0);
}

#[tokio::test]
async fn initial_bound_is_exclusive_and_excludes_current_message() {
    let log = FakeLog::new(vec![page(
        vec![message(9, Some("engineering"), None, "old")],
        None,
    )]);
    let mut query = query(5);
    query.before = Some(Sequence(10));
    let history = project_session(&log, &query).await.expect("projects");
    assert_eq!(history[0].sequence, Sequence(9));
    assert_eq!(
        log.calls.lock().expect("calls lock")[0].0,
        Some(Sequence(10))
    );
}

#[tokio::test]
async fn reads_multiple_pages_and_returns_chronological_window() {
    let log = FakeLog::new(vec![
        page(
            vec![
                message(6, Some("engineering"), None, "six"),
                message(5, Some("other"), None, "other"),
                message(4, Some("engineering"), None, "four"),
            ],
            Some(4),
        ),
        page(
            vec![
                message(3, Some("engineering"), None, "three"),
                message(2, Some("engineering"), None, "two"),
            ],
            None,
        ),
    ]);
    let history = project_session(&log, &query(4)).await.expect("projects");
    assert_eq!(
        history
            .iter()
            .map(|item| item.sequence.0)
            .collect::<Vec<_>>(),
        vec![2, 3, 4, 6]
    );
    assert_eq!(log.call_count(), 2);
}

#[tokio::test]
async fn rejects_a_row_at_or_above_the_exclusive_bound() {
    let log = FakeLog::new(vec![page(
        vec![message(10, Some("engineering"), None, "bad")],
        None,
    )]);
    let mut query = query(2);
    query.before = Some(Sequence(10));
    assert!(matches!(
        project_session(&log, &query).await,
        Err(Error::PageOutOfRange { .. })
    ));
}

#[tokio::test]
async fn rejects_rows_that_are_not_strictly_descending() {
    let log = FakeLog::new(vec![page(
        vec![message(3, None, None, "a"), message(4, None, None, "b")],
        None,
    )]);
    assert!(matches!(
        project_session(&log, &query(2)).await,
        Err(Error::PageNotDescending { .. })
    ));
}

#[tokio::test]
async fn rejects_duplicate_sequences() {
    let log = FakeLog::new(vec![page(
        vec![message(3, None, None, "a"), message(3, None, None, "b")],
        None,
    )]);
    assert!(matches!(
        project_session(&log, &query(2)).await,
        Err(Error::DuplicateSequence { .. })
    ));
}

#[tokio::test]
async fn rejects_an_empty_page_with_a_cursor() {
    let log = FakeLog::new(vec![page(Vec::new(), Some(4))]);
    assert!(matches!(
        project_session(&log, &query(2)).await,
        Err(Error::EmptyPageCursor { .. })
    ));
}

#[tokio::test]
async fn rejects_a_cursor_that_does_not_advance() {
    let log = FakeLog::new(vec![page(vec![message(9, None, None, "a")], Some(10))]);
    let mut query = query(2);
    query.before = Some(Sequence(10));
    assert!(matches!(
        project_session(&log, &query).await,
        Err(Error::CursorDidNotAdvance { .. })
    ));
}

#[tokio::test]
async fn accepts_a_cursor_older_than_the_oldest_row() {
    let log = FakeLog::new(vec![page(
        vec![message(9, None, None, "a"), message(8, None, None, "b")],
        Some(7),
    )]);
    assert!(project_session(&log, &query(2)).await.is_ok());
}

#[tokio::test]
async fn rejects_a_cursor_newer_than_the_oldest_row() {
    let log = FakeLog::new(vec![page(
        vec![message(9, None, None, "a"), message(8, None, None, "b")],
        Some(9),
    )]);
    assert!(matches!(
        project_session(&log, &query(2)).await,
        Err(Error::CursorAfterOldest { .. })
    ));
}

#[tokio::test]
async fn rejects_a_page_larger_than_the_requested_limit() {
    let messages = (0..=PAGE_SIZE as u64)
        .map(|offset| message(2_000 - offset, Some("other"), None, "row"))
        .collect();
    let log = FakeLog::new(vec![page(messages, None)]);
    assert!(matches!(
        project_session(&log, &query(1)).await,
        Err(Error::PageTooLarge {
            requested: PAGE_SIZE,
            actual
        }) if actual == PAGE_SIZE + 1
    ));
}

#[tokio::test]
async fn scan_cap_is_a_successful_partial_projection() {
    let mut pages = Vec::new();
    for page_index in 0_u64..4 {
        let high = 3_000 - page_index * PAGE_SIZE as u64;
        let messages = (0..PAGE_SIZE as u64)
            .map(|offset| message(high - offset, Some("other"), None, "ignored"))
            .collect::<Vec<_>>();
        pages.push(page(messages, Some(high - (PAGE_SIZE as u64 - 1))));
    }
    let log = FakeLog::new(pages);
    assert!(
        project_session(&log, &query(1))
            .await
            .expect("scan cap succeeds")
            .is_empty()
    );
    assert_eq!(log.call_count(), 4);
}

#[tokio::test]
async fn filters_by_exact_desk_id_or_name_and_general_aliases() {
    let log = FakeLog::new(vec![page(
        vec![
            message(5, Some("Engineering"), None, "name"),
            message(4, Some("engineering"), None, "id"),
            message(3, Some("ENGINEERING"), None, "wrong case"),
        ],
        None,
    )]);
    let history = project_session(&log, &query(5)).await.expect("projects");
    assert_eq!(
        history
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>(),
        vec!["id", "name"]
    );

    let general = FakeLog::new(vec![page(vec![message(2, None, None, "general")], None)]);
    let mut general_query = query(2);
    general_query.conversation.desk_id = "General".into();
    general_query.conversation.desk_name = "General".into();
    assert_eq!(
        project_session(&general, &general_query)
            .await
            .expect("general")
            .len(),
        1
    );
}

#[tokio::test]
async fn channel_projection_keeps_a_root_and_its_first_reply() {
    let log = FakeLog::new(vec![page(
        vec![
            message(4, Some("engineering"), Some(2), "second reply"),
            message(3, Some("engineering"), Some(2), "first reply"),
            message(2, Some("engineering"), None, "channel"),
        ],
        None,
    )]);
    let history = project_session(&log, &query(5)).await.expect("projects");
    assert_eq!(
        history
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>(),
        vec!["channel", "first reply"]
    );
}

#[tokio::test]
async fn channel_projection_promotes_one_reply_per_root_independently() {
    let log = FakeLog::new(vec![page(
        vec![
            message(6, Some("engineering"), Some(2), "late on first"),
            message(5, Some("engineering"), Some(3), "answer two"),
            message(4, Some("engineering"), Some(2), "answer one"),
            message(3, Some("engineering"), None, "question two"),
            message(2, Some("engineering"), None, "question one"),
        ],
        None,
    )]);
    let history = project_session(&log, &query(9)).await.expect("projects");
    assert_eq!(
        history
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>(),
        vec!["question one", "question two", "answer one", "answer two"]
    );
}

#[tokio::test]
async fn channel_projection_never_promotes_a_reply_to_a_reply() {
    let log = FakeLog::new(vec![page(
        vec![
            message(4, Some("engineering"), Some(3), "grandchild"),
            message(3, Some("engineering"), Some(2), "first reply"),
            message(2, Some("engineering"), None, "root"),
        ],
        None,
    )]);
    let history = project_session(&log, &query(5)).await.expect("projects");
    assert_eq!(
        history
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>(),
        vec!["root", "first reply"]
    );
}

#[tokio::test]
async fn channel_projection_drops_a_reply_whose_root_is_outside_the_scan() {
    let log = FakeLog::new(vec![page(
        vec![message(9, Some("engineering"), Some(2), "orphan reply")],
        None,
    )]);
    assert!(
        project_session(&log, &query(5))
            .await
            .expect("projects")
            .is_empty()
    );
}

#[tokio::test]
async fn channel_projection_promotes_past_an_empty_first_reply_and_an_empty_root() {
    let log = FakeLog::new(vec![page(
        vec![
            message(5, Some("engineering"), Some(4), "reply to a blank root"),
            message(4, Some("engineering"), None, "  \n "),
            message(3, Some("engineering"), Some(1), "the reply that counts"),
            message(2, Some("engineering"), Some(1), " \t "),
            message(1, Some("engineering"), None, "root"),
        ],
        None,
    )]);
    let history = project_session(&log, &query(9)).await.expect("projects");
    assert_eq!(
        history
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>(),
        vec!["root", "the reply that counts", "reply to a blank root"]
    );
}

#[tokio::test]
async fn channel_window_counts_what_survives_narrowing_not_rows_read() {
    let mut rows = vec![message(1, Some("engineering"), None, "root")];
    rows.extend((2..=40).map(|sequence| {
        message(
            sequence,
            Some("engineering"),
            Some(1),
            &format!("reply {sequence}"),
        )
    }));
    rows.reverse();
    let log = FakeLog::new(vec![page(rows, None)]);
    let history = project_session(&log, &query(2)).await.expect("projects");
    assert_eq!(
        history
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>(),
        vec!["root", "reply 2"]
    );
}

#[tokio::test]
async fn channel_window_keeps_the_newest_survivors() {
    let log = FakeLog::new(vec![page(
        vec![
            message(4, Some("engineering"), Some(3), "newest reply"),
            message(3, Some("engineering"), None, "newest root"),
            message(2, Some("engineering"), Some(1), "oldest reply"),
            message(1, Some("engineering"), None, "oldest root"),
        ],
        None,
    )]);
    let history = project_session(&log, &query(2)).await.expect("projects");
    assert_eq!(
        history
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>(),
        vec!["newest root", "newest reply"]
    );
}

#[tokio::test]
async fn channel_projection_stops_reading_once_the_window_is_met() {
    let log = FakeLog::new(vec![
        page(
            vec![
                message(4, Some("engineering"), Some(3), "reply"),
                message(3, Some("engineering"), None, "root"),
            ],
            Some(3),
        ),
        page(vec![message(2, Some("engineering"), None, "older")], None),
    ]);
    assert_eq!(
        project_session(&log, &query(2))
            .await
            .expect("projects")
            .len(),
        2
    );
    assert_eq!(log.call_count(), 1);
}

#[tokio::test]
async fn thread_keeps_root_and_direct_children_then_stops_at_root() {
    let log = FakeLog::new(vec![page(
        vec![
            message(8, Some("engineering"), Some(6), "nested elsewhere"),
            message(7, Some("engineering"), Some(5), "child"),
            message(6, Some("engineering"), Some(5), "child two"),
            message(5, Some("engineering"), None, "root"),
            message(4, Some("engineering"), None, "older"),
        ],
        None,
    )]);
    let mut query = query(8);
    query.conversation.thread_root = Some(Sequence(5));
    let history = project_session(&log, &query).await.expect("projects");
    assert_eq!(
        history
            .iter()
            .map(|item| item.sequence.0)
            .collect::<Vec<_>>(),
        vec![5, 6, 7]
    );
}

fn thread_query(window: usize, root: u64) -> SessionQuery {
    let mut query = query(window);
    query.conversation.thread_root = Some(Sequence(root));
    query
}

#[tokio::test]
async fn thread_projection_skips_a_blank_reply_and_stops_without_a_cursor() {
    let log = FakeLog::new(vec![page(
        vec![
            message(9, Some("engineering"), Some(5), "later"),
            message(8, Some("engineering"), Some(5), "   "),
            message(7, Some("engineering"), Some(5), "earlier"),
        ],
        None,
    )]);
    let history = project_session(&log, &thread_query(5, 5))
        .await
        .expect("projects");
    assert_eq!(
        history
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>(),
        vec!["earlier", "later"]
    );
    assert_eq!(log.call_count(), 1);
}

#[tokio::test]
async fn thread_projection_stops_reading_once_the_window_is_met() {
    let log = FakeLog::new(vec![
        page(
            vec![
                message(9, Some("engineering"), Some(5), "third"),
                message(8, Some("engineering"), Some(5), "second"),
                message(7, Some("engineering"), Some(5), "first"),
            ],
            Some(7),
        ),
        page(vec![message(5, Some("engineering"), None, "root")], None),
    ]);
    let history = project_session(&log, &thread_query(2, 5))
        .await
        .expect("projects");
    assert_eq!(
        history
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>(),
        vec!["second", "third"]
    );
    assert_eq!(log.call_count(), 1);
}

#[tokio::test]
async fn thread_projection_follows_the_cursor_across_pages_to_its_root() {
    let log = FakeLog::new(vec![
        page(
            vec![message(9, Some("engineering"), Some(5), "reply")],
            Some(9),
        ),
        page(
            vec![
                message(7, Some("other"), None, "elsewhere"),
                message(5, Some("engineering"), None, "root"),
            ],
            Some(5),
        ),
    ]);
    let history = project_session(&log, &thread_query(5, 5))
        .await
        .expect("projects");
    assert_eq!(
        history
            .iter()
            .map(|item| item.content.as_str())
            .collect::<Vec<_>>(),
        vec!["root", "reply"]
    );
    assert_eq!(log.call_count(), 2);
}

#[tokio::test]
async fn thread_scan_cap_is_a_successful_partial_projection() {
    let mut pages = Vec::new();
    for page_index in 0_u64..4 {
        let high = 3_000 - page_index * PAGE_SIZE as u64;
        let messages = (0..PAGE_SIZE as u64)
            .map(|offset| message(high - offset, Some("other"), None, "ignored"))
            .collect::<Vec<_>>();
        pages.push(page(messages, Some(high - (PAGE_SIZE as u64 - 1))));
    }
    let log = FakeLog::new(pages);
    assert!(
        project_session(&log, &thread_query(1, 5))
            .await
            .expect("scan cap succeeds")
            .is_empty()
    );
    assert_eq!(log.call_count(), 4);
}

#[tokio::test]
async fn thread_projection_reports_read_and_validation_failures() {
    let error = project_session(&FakeLog::failing(), &thread_query(2, 5))
        .await
        .expect_err("read fails");
    assert!(matches!(error, Error::Read { .. }));

    let oversized = (0..=PAGE_SIZE as u64)
        .map(|offset| message(2_000 - offset, Some("other"), None, "row"))
        .collect();
    let log = FakeLog::new(vec![page(oversized, None)]);
    assert!(matches!(
        project_session(&log, &thread_query(1, 5)).await,
        Err(Error::PageTooLarge { .. })
    ));
}

#[tokio::test]
async fn whitespace_thread_root_stops_before_another_read() {
    let log = FakeLog {
        pages: Mutex::new(VecDeque::from([
            Ok(page(
                vec![message(5, Some("engineering"), None, " \n\t ")],
                Some(5),
            )),
            Err(Box::new(io::Error::other("must not read past root")) as SourceError),
        ])),
        calls: Arc::default(),
    };
    let mut query = query(8);
    query.conversation.thread_root = Some(Sequence(5));
    assert!(
        project_session(&log, &query)
            .await
            .expect("root terminates the walk")
            .is_empty()
    );
    assert_eq!(log.call_count(), 1);
}

#[tokio::test]
async fn skips_trim_empty_content_but_preserves_other_bytes_and_author() {
    let author = SessionAuthor::Person {
        id: "p1".into(),
        label: "Pat".into(),
    };
    let mut kept = message(4, Some("engineering"), None, "  keep me  \n");
    kept.author = author.clone();
    let log = FakeLog::new(vec![page(
        vec![kept, message(3, Some("engineering"), None, " \n\t ")],
        None,
    )]);
    let history = project_session(&log, &query(3)).await.expect("projects");
    assert_eq!(history[0].content, "  keep me  \n");
    assert_eq!(history[0].author, author);
}

#[tokio::test]
async fn propagates_source_errors_with_their_source() {
    let error = project_session(&FakeLog::failing(), &query(2))
        .await
        .expect_err("read fails");
    assert!(matches!(error, Error::Read { .. }));
    assert!(std::error::Error::source(&error).is_some());
}
